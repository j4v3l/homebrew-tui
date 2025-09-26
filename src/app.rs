// Minimal, clean App implementation with SearchResults and Help modal.
// We'll expand features (confirm modal, input prompt, logs) once the repo is stable.

use crate::brew::{Brew, FormulaInfo};
use crate::ui::draw_ui;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::process::Command as ProcessCommand;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug)]
pub enum InputAction {
    Install,
    Search,
}

#[derive(Clone, Debug)]
pub enum ConfirmAction {
    Install,
    Uninstall,
    Upgrade,
    BulkUpgrade(Vec<String>),
    InstallBrew,
}

#[derive(Clone, Debug)]
pub enum Mode {
    Normal,
    Help,
    Input {
        action: InputAction,
        buffer: String,
    },
    Confirm {
        action: ConfirmAction,
        name: String,
        idx: Option<usize>,
    },
    SearchResults {
        results: Vec<String>,
        selected: usize,
    },
    Outdated {
        packages: Vec<String>,
        cursor: usize,
        checked: Vec<bool>,
        scroll: usize,
    },
    Operation {
        title: String,
        logs: Vec<String>,
        scroll: usize,
    },
}

#[derive(Clone, PartialEq)]
pub enum Focus {
    Installed,
    Available,
}

pub enum AppEvent {
    Status(String),
    BrewList(Vec<FormulaInfo>),
    BrewInfo(FormulaInfo, usize),
    BrewInfoAvailable(FormulaInfo, usize),
    Log(String),
    OpStart(String),
    OpLog(String),
    OpEnd(String),
    ShowConfirm(ConfirmAction, String, Option<usize>),
    SearchResults(Vec<String>),
    OutdatedList(Vec<String>),
    AvailableList(Vec<String>),
}

pub struct App {
    pub brew: Brew,
    pub items: Vec<FormulaInfo>,
    pub available_items: Vec<String>,
    pub outdated_items: Vec<String>,
    pub selected: usize,
    pub available_selected: usize,
    pub last_selected: Option<(Focus, usize)>,
    pub available_details: Option<FormulaInfo>,
    pub available_filter: String,
    pub available_filtered: Vec<usize>,
    pub last_refreshed: Option<SystemTime>,
    pub operation_status: Option<String>,
    pub operation_percent: Option<u16>,
    pub spinner_idx: usize,
    pub loading_installed: bool,
    pub loading_available: bool,
    pub operating: bool,
    pub status: String,
    pub logs: Vec<String>,
    pub rx: mpsc::Receiver<AppEvent>,
    pub tx: mpsc::Sender<AppEvent>,
    pub mode: Mode,
    pub focus: Focus,
}

impl App {
    pub fn new() -> Result<Self> {
        let brew = Brew::new();
        let (tx, rx) = mpsc::channel();

        // background loader for installed
        let tx_bg = tx.clone();
        let brew_bg = brew.clone();
        thread::spawn(move || {
            let _ = tx_bg.send(AppEvent::Status("loading installed".to_string()));
            if let Ok(list) = brew_bg.list_installed() {
                let _ = tx_bg.send(AppEvent::BrewList(list));
            }
        });

        // background loader for all available packages
        let tx_av = tx.clone();
        let brew_av = brew.clone();
        thread::spawn(move || {
            let _ = tx_av.send(AppEvent::Status("loading available".to_string()));
            if let Ok(list) = brew_av.all_available() {
                let _ = tx_av.send(AppEvent::AvailableList(list));
            }
        });

        // background loader for outdated (upgradable) installed packages
        let tx_out = tx.clone();
        let brew_out = brew.clone();
        thread::spawn(move || {
            let _ = tx_out.send(AppEvent::Status("checking for updates".to_string()));
            match brew_out.outdated() {
                Ok(list) => {
                    let _ = tx_out.send(AppEvent::OutdatedList(list));
                }
                Err(e) => {
                    let _ = tx_out.send(AppEvent::Log(format!("outdated check failed: {}", e)));
                }
            }
        });

        // periodic refresher: re-run outdated every 5 minutes
        let tx_periodic = tx.clone();
        let brew_periodic = brew.clone();
        thread::spawn(move || loop {
            thread::sleep(Duration::from_secs(300));
            match brew_periodic.outdated() {
                Ok(list) => {
                    let _ = tx_periodic.send(AppEvent::OutdatedList(list));
                }
                Err(e) => {
                    let _ =
                        tx_periodic.send(AppEvent::Log(format!("outdated check failed: {}", e)));
                }
            }
        });

        // Start in Normal mode, but if brew is missing, send a delayed ShowConfirm event so
        // the UI is drawn once before the prompt appears.
        let initial_mode = Mode::Normal;
        // detect brew presence and post a ShowConfirm after a short delay if missing
        let tx_detect = tx.clone();
        thread::spawn(move || {
            if ProcessCommand::new("brew")
                .arg("--version")
                .output()
                .is_err()
            {
                // give the UI a chance to render once
                thread::sleep(Duration::from_millis(250));
                let _ = tx_detect.send(AppEvent::ShowConfirm(
                    ConfirmAction::InstallBrew,
                    "Homebrew".into(),
                    None,
                ));
            }
        });

        Ok(Self {
            brew,
            items: vec![],
            available_items: vec![],
            outdated_items: vec![],
            selected: 0,
            available_selected: 0,
            last_selected: None,
            available_details: None,
            available_filter: String::new(),
            available_filtered: vec![],
            last_refreshed: None,
            operation_status: None,
            operation_percent: None,
            spinner_idx: 0,
            loading_installed: true,
            loading_available: true,
            operating: false,
            status: "Starting...".into(),
            logs: vec![],
            rx,
            tx,
            mode: initial_mode,
            focus: Focus::Installed,
        })
    }

    /// Handle a single `AppEvent`. This is extracted from the body of the main run loop so
    /// tests can exercise event handling without running the full UI loop or background threads.
    pub fn handle_event(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::BrewList(list) => {
                self.items = list;
                self.status = format!("Loaded {} packages", self.items.len());
                self.last_refreshed = Some(SystemTime::now());
                self.loading_installed = false;
                self.last_selected = None;
                // refresh outdated list whenever installed list changes
                let tx = self.tx.clone();
                let brew = self.brew.clone();
                thread::spawn(move || match brew.outdated() {
                    Ok(list) => {
                        let _ = tx.send(AppEvent::OutdatedList(list));
                    }
                    Err(e) => {
                        let _ = tx.send(AppEvent::Log(format!("outdated check failed: {}", e)));
                    }
                });
            }
            AppEvent::BrewInfo(info, idx) => {
                if idx < self.items.len() {
                    self.items[idx] = info;
                }
                self.last_selected = Some((Focus::Installed, idx));
            }
            AppEvent::BrewInfoAvailable(info, idx) => {
                self.available_details = Some(info);
                self.last_selected = Some((Focus::Available, idx));
            }
            AppEvent::Status(s) => self.status = s,
            AppEvent::OutdatedList(list) => self.outdated_items = list,
            AppEvent::Log(l) => self.push_log(l),
            AppEvent::OpStart(title) => {
                self.mode = Mode::Operation {
                    title: title.clone(),
                    logs: vec![],
                    scroll: 0,
                };
                self.push_log(format!("Started: {}", title));
                self.operating = true;
                self.operation_percent = None;
            }
            AppEvent::OpLog(line) => {
                if let Mode::Operation { logs, scroll, .. } = &mut self.mode {
                    logs.push(line.clone());
                    if *scroll > 0 {}
                    if logs.len() > 2000 {
                        logs.drain(0..500);
                        if *scroll > logs.len() {
                            *scroll = logs.len();
                        }
                    }
                }
                let line_clone = line.clone();
                self.push_log(line_clone.clone());
                if let Some(pct) = Self::parse_percent(&line_clone) {
                    self.operation_status = Some(format!("{}%", pct));
                    self.operation_percent = Some(pct);
                }
            }
            AppEvent::OpEnd(title) => {
                self.push_log(format!("Finished: {}", title));
                self.operation_status = None;
                self.operation_percent = None;
                self.operating = false;
            }
            AppEvent::SearchResults(results) => {
                self.mode = Mode::SearchResults {
                    results,
                    selected: 0,
                };
            }
            AppEvent::AvailableList(list) => {
                self.available_items = list;
                self.status = format!("Loaded {} available packages", self.available_items.len());
                self.last_refreshed = Some(SystemTime::now());
                self.loading_available = false;
                self.available_filtered = (0..self.available_items.len()).collect();
            }
            AppEvent::ShowConfirm(action, name, idx) => {
                self.mode = Mode::Confirm { action, name, idx };
            }
        }
    }

    fn push_log(&mut self, s: String) {
        self.logs.push(s);
        if self.logs.len() > 300 {
            self.logs.drain(0..100);
        }
    }

    /// Try to extract a percentage value from a free-form log line.
    /// Scans for patterns like "(\d+)%" or digits followed by % and returns 0..=100.
    fn parse_percent(s: &str) -> Option<u16> {
        // find the first '%' and then scan backwards for contiguous digits
        if let Some(pos) = s.find('%') {
            // scan left from pos-1
            let mut i = pos as isize - 1;
            let mut digits = String::new();
            while i >= 0 {
                let c = s.as_bytes()[i as usize] as char;
                if c.is_ascii_digit() {
                    digits.insert(0, c);
                    i -= 1;
                } else {
                    break;
                }
            }
            if !digits.is_empty() {
                if let Ok(mut v) = digits.parse::<u16>() {
                    if v > 100 {
                        v = 100;
                    }
                    return Some(v);
                }
            }
        }
        None
    }

    pub fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ) -> Result<()> {
        loop {
            // drain events from background
            while let Ok(ev) = self.rx.try_recv() {
                match ev {
                    AppEvent::BrewList(list) => {
                        self.items = list;
                        self.status = format!("Loaded {} packages", self.items.len());
                        self.last_refreshed = Some(SystemTime::now());
                        self.loading_installed = false;
                        self.last_selected = None;
                        // refresh outdated list whenever installed list changes
                        let tx = self.tx.clone();
                        let brew = self.brew.clone();
                        thread::spawn(move || match brew.outdated() {
                            Ok(list) => {
                                let _ = tx.send(AppEvent::OutdatedList(list));
                            }
                            Err(e) => {
                                let _ =
                                    tx.send(AppEvent::Log(format!("outdated check failed: {}", e)));
                            }
                        });
                    }
                    AppEvent::BrewInfo(info, idx) => {
                        if idx < self.items.len() {
                            self.items[idx] = info;
                        }
                        self.last_selected = Some((Focus::Installed, idx));
                    }
                    AppEvent::BrewInfoAvailable(info, idx) => {
                        self.available_details = Some(info);
                        self.last_selected = Some((Focus::Available, idx));
                    }
                    AppEvent::Status(s) => self.status = s,
                    AppEvent::OutdatedList(list) => self.outdated_items = list,
                    AppEvent::Log(l) => self.push_log(l),
                    AppEvent::OpStart(title) => {
                        self.mode = Mode::Operation {
                            title: title.clone(),
                            logs: vec![],
                            scroll: 0,
                        };
                        self.push_log(format!("Started: {}", title));
                        self.operating = true;
                        self.operation_percent = None;
                    }
                    AppEvent::OpLog(line) => {
                        if let Mode::Operation { logs, scroll, .. } = &mut self.mode {
                            logs.push(line.clone());
                            if *scroll > 0 {
                                // user scrolled up; do not auto-scroll
                            }
                            if logs.len() > 2000 {
                                logs.drain(0..500);
                                if *scroll > logs.len() {
                                    *scroll = logs.len();
                                }
                            }
                        }
                        let line_clone = line.clone();
                        self.push_log(line_clone.clone());
                        if let Some(pct_pos) = line_clone.find('%') {
                            let start = if pct_pos >= 4 { pct_pos - 4 } else { 0 };
                            let candidate = &line_clone[start..pct_pos + 1];
                            let filtered: String = candidate
                                .chars()
                                .filter(|c| c.is_digit(10) || *c == '%')
                                .collect();
                            if filtered.ends_with('%') {
                                let digits: String =
                                    filtered.chars().filter(|c| c.is_digit(10)).collect();
                                if let Ok(pct) = digits.parse::<u16>() {
                                    let p = if pct > 100 { 100 } else { pct };
                                    self.operation_status = Some(format!("{}%", p));
                                    self.operation_percent = Some(p);
                                }
                            }
                        }
                    }
                    AppEvent::OpEnd(title) => {
                        self.push_log(format!("Finished: {}", title));
                        self.operation_status = None;
                        self.operation_percent = None;
                        self.operating = false;
                    }
                    AppEvent::SearchResults(results) => {
                        self.mode = Mode::SearchResults {
                            results,
                            selected: 0,
                        };
                    }
                    AppEvent::AvailableList(list) => {
                        self.available_items = list;
                        self.status =
                            format!("Loaded {} available packages", self.available_items.len());
                        self.last_refreshed = Some(SystemTime::now());
                        self.loading_available = false;
                        self.available_filtered = (0..self.available_items.len()).collect();
                    }
                    AppEvent::ShowConfirm(action, name, idx) => {
                        self.mode = Mode::Confirm { action, name, idx };
                    }
                }
            }

            // advance spinner frame each tick
            self.spinner_idx = (self.spinner_idx + 1) % 8;

            // build a richer status line for the bottom-right Status pane
            let mode_str = match &self.mode {
                Mode::Normal => "Normal".to_string(),
                Mode::Help => "Help".to_string(),
                Mode::Input { action, .. } => match action {
                    InputAction::Install => "Input(Install)".to_string(),
                    InputAction::Search => "Input(Search)".to_string(),
                },
                Mode::Confirm { action, name, .. } => match action {
                    ConfirmAction::Install => format!("Confirm Install {}", name),
                    ConfirmAction::Uninstall => format!("Confirm Uninstall {}", name),
                    ConfirmAction::Upgrade => format!("Confirm Upgrade {}", name),
                    ConfirmAction::BulkUpgrade(_) => format!("Confirm Bulk Upgrade {}", name),
                    ConfirmAction::InstallBrew => format!("Confirm Install Homebrew"),
                },
                Mode::SearchResults { results, selected } => {
                    format!("SearchResults {} results (sel {})", results.len(), selected)
                }
                Mode::Outdated {
                    packages, cursor, ..
                } => format!("Outdated {} packages (cursor {})", packages.len(), cursor),
                Mode::Operation { title, logs, .. } => {
                    format!("Operation: {} ({} lines)", title, logs.len())
                }
            };

            let focus_str = match &self.focus {
                Focus::Installed => "Installed",
                Focus::Available => "Available",
            };

            // determine selected name and whether it's installed
            let selected_name = if self.focus == Focus::Installed {
                self.items
                    .get(self.selected)
                    .map(|f| f.name.clone())
                    .unwrap_or_default()
            } else {
                self.available_items
                    .get(self.available_selected)
                    .cloned()
                    .unwrap_or_default()
            };

            let selected_installed = if selected_name.is_empty() {
                String::new()
            } else {
                let installed = self.items.iter().any(|f| f.name == selected_name);
                if installed {
                    "(installed)".to_string()
                } else {
                    "(not installed)".to_string()
                }
            };

            let recent_logs = self.logs.len();

            self.status = format!(
                "Installed: {}  Available: {}  Focus: {}  Selected: {} {}  Mode: {}  Logs: {}",
                self.items.len(),
                self.available_items.len(),
                focus_str,
                if selected_name.is_empty() {
                    "-"
                } else {
                    &selected_name
                },
                selected_installed,
                mode_str,
                recent_logs
            );

            draw_ui(terminal, self)?;

            if event::poll(std::time::Duration::from_millis(200))? {
                if let Event::Key(key) = event::read()? {
                    // Help modal
                    if let Mode::Help = &self.mode {
                        let mode_taken = std::mem::replace(&mut self.mode, Mode::Normal);
                        if let Mode::Help = mode_taken {
                            match key.code {
                                KeyCode::Char('?') | KeyCode::Esc => {}
                                _ => {}
                            }
                        }
                    // Operation modal handling
                    } else if let Mode::Operation { .. } = &self.mode {
                        let mode_taken = std::mem::replace(&mut self.mode, Mode::Normal);
                        if let Mode::Operation {
                            title,
                            logs,
                            mut scroll,
                        } = mode_taken
                        {
                            match key.code {
                                KeyCode::Esc | KeyCode::Char('?') => {}
                                KeyCode::Up | KeyCode::Char('k') => {
                                    if scroll + 1 < logs.len() {
                                        scroll = scroll.saturating_add(1);
                                    } else {
                                        scroll = logs.len();
                                    }
                                    self.mode = Mode::Operation {
                                        title,
                                        logs,
                                        scroll,
                                    };
                                }
                                KeyCode::Down | KeyCode::Char('j') => {
                                    if scroll > 0 {
                                        scroll = scroll.saturating_sub(1);
                                    }
                                    self.mode = Mode::Operation {
                                        title,
                                        logs,
                                        scroll,
                                    };
                                }
                                KeyCode::PageUp => {
                                    let page = 10usize;
                                    scroll = (scroll + page).min(logs.len());
                                    self.mode = Mode::Operation {
                                        title,
                                        logs,
                                        scroll,
                                    };
                                }
                                KeyCode::PageDown => {
                                    let page = 10usize;
                                    scroll = scroll.saturating_sub(page);
                                    self.mode = Mode::Operation {
                                        title,
                                        logs,
                                        scroll,
                                    };
                                }
                                KeyCode::Home => {
                                    scroll = logs.len();
                                    self.mode = Mode::Operation {
                                        title,
                                        logs,
                                        scroll,
                                    };
                                }
                                KeyCode::End => {
                                    scroll = 0;
                                    self.mode = Mode::Operation {
                                        title,
                                        logs,
                                        scroll,
                                    };
                                }
                                _ => {
                                    self.mode = Mode::Operation {
                                        title,
                                        logs,
                                        scroll,
                                    };
                                }
                            }
                        }
                    // Confirm modal handling (with InstallBrew special-case)
                    } else if let Mode::Confirm { .. } = &self.mode {
                        let mode_taken = std::mem::replace(&mut self.mode, Mode::Normal);
                        if let Mode::Confirm {
                            action,
                            name,
                            idx: _,
                        } = mode_taken
                        {
                            match key.code {
                                KeyCode::Char('y') | KeyCode::Enter => {
                                    let action = action.clone();
                                    let name = name.clone();
                                    let tx = self.tx.clone();
                                    thread::spawn(move || {
                                        use std::io::{BufRead, BufReader};
                                        use std::process::{Command, Stdio};

                                        // Special-case Homebrew installation
                                        if let ConfirmAction::InstallBrew = action {
                                            let title = "install-homebrew".to_string();
                                            let _ = tx.send(AppEvent::OpStart(title.clone()));
                                            match Command::new("/bin/bash")
                                                .arg("-lc")
                                                .arg("/bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\"")
                                                .stdout(Stdio::piped())
                                                .stderr(Stdio::piped())
                                                .spawn()
                                            {
                                                Ok(mut child) => {
                                                    if let Some(stdout) = child.stdout.take() {
                                                        let txo = tx.clone();
                                                        thread::spawn(move || {
                                                            let reader = BufReader::new(stdout);
                                                            for line in reader.lines() { if let Ok(l) = line { let _ = txo.send(AppEvent::OpLog(l)); } }
                                                        });
                                                    }
                                                    if let Some(stderr) = child.stderr.take() {
                                                        let txe = tx.clone();
                                                        thread::spawn(move || {
                                                            let reader = BufReader::new(stderr);
                                                            for line in reader.lines() { if let Ok(l) = line { let _ = txe.send(AppEvent::OpLog(l)); } }
                                                        });
                                                    }
                                                    match child.wait() {
                                                        Ok(status) => {
                                                            if status.success() {
                                                                let _ = tx.send(AppEvent::Status(format!("{} completed", title)));
                                                            } else {
                                                                let _ = tx.send(AppEvent::Log(format!("{} failed: {}", title, status)));
                                                            }
                                                        }
                                                        Err(e) => { let _ = tx.send(AppEvent::Log(format!("failed waiting for installer: {}", e))); }
                                                    }
                                                }
                                                Err(e) => { let _ = tx.send(AppEvent::Log(format!("failed to spawn installer: {}", e))); }
                                            }
                                            let _ = tx.send(AppEvent::OpEnd(title));
                                            return;
                                        }

                                        // Otherwise handle brew verbs normally
                                        let (verb, args) = match action {
                                            ConfirmAction::Uninstall => {
                                                ("uninstall", vec![name.clone()])
                                            }
                                            ConfirmAction::Upgrade => {
                                                ("upgrade", vec![name.clone()])
                                            }
                                            ConfirmAction::Install => {
                                                ("install", vec![name.clone()])
                                            }
                                            ConfirmAction::BulkUpgrade(pkgs) => {
                                                ("upgrade", pkgs.clone())
                                            }
                                            _ => ("", vec![]),
                                        };

                                        let title = if verb.is_empty() {
                                            "brew op".to_string()
                                        } else {
                                            format!("brew {} {}", verb, args.join(" "))
                                        };
                                        let _ = tx.send(AppEvent::OpStart(title.clone()));

                                        match Command::new("brew")
                                            .arg(verb)
                                            .args(&args)
                                            .stdout(Stdio::piped())
                                            .stderr(Stdio::piped())
                                            .spawn()
                                        {
                                            Ok(mut child) => {
                                                if let Some(stdout) = child.stdout.take() {
                                                    let tx_out = tx.clone();
                                                    thread::spawn(move || {
                                                        let reader = BufReader::new(stdout);
                                                        for line in reader.lines() {
                                                            if let Ok(l) = line {
                                                                let _ =
                                                                    tx_out.send(AppEvent::OpLog(l));
                                                            }
                                                        }
                                                    });
                                                }
                                                if let Some(stderr) = child.stderr.take() {
                                                    let tx_err = tx.clone();
                                                    thread::spawn(move || {
                                                        let reader = BufReader::new(stderr);
                                                        for line in reader.lines() {
                                                            if let Ok(l) = line {
                                                                let _ =
                                                                    tx_err.send(AppEvent::OpLog(l));
                                                            }
                                                        }
                                                    });
                                                }
                                                match child.wait() {
                                                    Ok(status) => {
                                                        if status.success() {
                                                            let _ = tx.send(AppEvent::Status(
                                                                format!("{} completed", title),
                                                            ));
                                                            if let Ok(list) =
                                                                Brew::new().list_installed()
                                                            {
                                                                let _ = tx
                                                                    .send(AppEvent::BrewList(list));
                                                            }
                                                        } else {
                                                            let _ =
                                                                tx.send(AppEvent::Log(format!(
                                                                    "{} failed: {}",
                                                                    title, status
                                                                )));
                                                        }
                                                    }
                                                    Err(e) => {
                                                        let _ = tx.send(AppEvent::OpLog(format!(
                                                            "failed waiting for brew: {}",
                                                            e
                                                        )));
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                let _ = tx.send(AppEvent::OpLog(format!(
                                                    "failed to spawn brew: {}",
                                                    e
                                                )));
                                            }
                                        }

                                        let _ = tx.send(AppEvent::OpEnd(title));
                                    });
                                }
                                KeyCode::Char('n') | KeyCode::Esc => {
                                    self.status = "Cancelled".into();
                                }
                                _ => {}
                            }
                        }
                    // Input modal handling (take ownership then reapply)
                    } else if let Mode::Input { .. } = &self.mode {
                        let mode_taken = std::mem::replace(&mut self.mode, Mode::Normal);
                        if let Mode::Input { action, mut buffer } = mode_taken {
                            match key.code {
                                KeyCode::Esc => {
                                    self.status = "Cancelled input".into();
                                }
                                KeyCode::Backspace => {
                                    buffer.pop();
                                    if let InputAction::Search = action {
                                        self.available_filter = buffer.clone();
                                        self.available_filtered = self
                                            .available_items
                                            .iter()
                                            .enumerate()
                                            .filter_map(|(i, name)| {
                                                if name.contains(&self.available_filter) {
                                                    Some(i)
                                                } else {
                                                    None
                                                }
                                            })
                                            .collect();
                                    }
                                    self.mode = Mode::Input { action, buffer };
                                }
                                KeyCode::Enter => {
                                    let value = buffer.trim().to_string();
                                    if !value.is_empty() {
                                        match action {
                                            InputAction::Install => {
                                                let name = value.clone();
                                                self.mode = Mode::Confirm {
                                                    action: ConfirmAction::Install,
                                                    name,
                                                    idx: None,
                                                };
                                            }
                                            InputAction::Search => {
                                                if self.focus == Focus::Available {
                                                    self.available_filter = value.clone();
                                                    self.available_filtered = self
                                                        .available_items
                                                        .iter()
                                                        .enumerate()
                                                        .filter_map(|(i, name)| {
                                                            if name.contains(&self.available_filter)
                                                            {
                                                                Some(i)
                                                            } else {
                                                                None
                                                            }
                                                        })
                                                        .collect();
                                                    if let Some(&idx) =
                                                        self.available_filtered.first()
                                                    {
                                                        self.available_selected = idx;
                                                    }
                                                } else {
                                                    let query = value.clone();
                                                    let tx = self.tx.clone();
                                                    let brew = self.brew.clone();
                                                    thread::spawn(move || {
                                                        match brew.search(&query) {
                                                            Ok(results) => {
                                                                let _ = tx.send(
                                                                    AppEvent::SearchResults(
                                                                        results,
                                                                    ),
                                                                );
                                                            }
                                                            Err(e) => {
                                                                let _ = tx.send(AppEvent::Log(
                                                                    format!("Search failed: {}", e),
                                                                ));
                                                            }
                                                        }
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                                KeyCode::Char(c) => {
                                    buffer.push(c);
                                    if let InputAction::Search = action {
                                        self.available_filter = buffer.clone();
                                        self.available_filtered = self
                                            .available_items
                                            .iter()
                                            .enumerate()
                                            .filter_map(|(i, name)| {
                                                if name.contains(&self.available_filter) {
                                                    Some(i)
                                                } else {
                                                    None
                                                }
                                            })
                                            .collect();
                                        if let Some(&idx) = self.available_filtered.first() {
                                            self.available_selected = idx;
                                        }
                                    }
                                    self.mode = Mode::Input { action, buffer };
                                }
                                _ => {
                                    self.mode = Mode::Input { action, buffer };
                                }
                            }
                        }
                    // Search results modal handling
                    } else if let Mode::SearchResults { .. } = &self.mode {
                        let mode_taken = std::mem::replace(&mut self.mode, Mode::Normal);
                        if let Mode::SearchResults {
                            results,
                            mut selected,
                        } = mode_taken
                        {
                            match key.code {
                                KeyCode::Up | KeyCode::Char('k') => {
                                    if selected > 0 {
                                        selected -= 1;
                                    }
                                    self.mode = Mode::SearchResults { results, selected };
                                }
                                KeyCode::Down | KeyCode::Char('j') => {
                                    if selected + 1 < results.len() {
                                        selected += 1;
                                    }
                                    self.mode = Mode::SearchResults { results, selected };
                                }
                                KeyCode::Enter => {
                                    if let Some(name) = results.get(selected).cloned() {
                                        self.mode = Mode::Confirm {
                                            action: ConfirmAction::Install,
                                            name,
                                            idx: None,
                                        };
                                    } else {
                                        self.mode = Mode::Normal;
                                    }
                                }
                                KeyCode::Esc | KeyCode::Char('?') => {
                                    self.mode = Mode::Normal;
                                }
                                _ => {
                                    self.mode = Mode::SearchResults { results, selected };
                                }
                            }
                        }
                    } else if let Mode::Outdated { .. } = &self.mode {
                        // Outdated modal handling
                        let mode_taken = std::mem::replace(&mut self.mode, Mode::Normal);
                        if let Mode::Outdated {
                            packages,
                            mut cursor,
                            mut checked,
                            scroll,
                        } = mode_taken
                        {
                            match key.code {
                                KeyCode::Esc | KeyCode::Char('?') => {}
                                KeyCode::Up | KeyCode::Char('k') => {
                                    if cursor > 0 {
                                        cursor -= 1
                                    }
                                    self.mode = Mode::Outdated {
                                        packages,
                                        cursor,
                                        checked,
                                        scroll,
                                    };
                                }
                                KeyCode::Down | KeyCode::Char('j') => {
                                    if cursor + 1 < packages.len() {
                                        cursor += 1
                                    }
                                    self.mode = Mode::Outdated {
                                        packages,
                                        cursor,
                                        checked,
                                        scroll,
                                    };
                                }
                                KeyCode::Char(' ') => {
                                    if cursor < checked.len() {
                                        checked[cursor] = !checked[cursor];
                                    }
                                    self.mode = Mode::Outdated {
                                        packages,
                                        cursor,
                                        checked,
                                        scroll,
                                    };
                                }
                                KeyCode::Enter => {
                                    let to_upgrade: Vec<String> = packages
                                        .iter()
                                        .enumerate()
                                        .filter_map(|(i, p)| {
                                            if checked.get(i).copied().unwrap_or(false) {
                                                Some(p.clone())
                                            } else {
                                                None
                                            }
                                        })
                                        .collect();
                                    if !to_upgrade.is_empty() {
                                        let name = if to_upgrade.len() == 1 {
                                            to_upgrade[0].clone()
                                        } else {
                                            format!("{} packages", to_upgrade.len())
                                        };
                                        self.mode = Mode::Confirm {
                                            action: ConfirmAction::BulkUpgrade(to_upgrade),
                                            name,
                                            idx: None,
                                        };
                                    } else {
                                        self.mode = Mode::Outdated {
                                            packages,
                                            cursor,
                                            checked,
                                            scroll,
                                        };
                                    }
                                }
                                _ => {
                                    self.mode = Mode::Outdated {
                                        packages,
                                        cursor,
                                        checked,
                                        scroll,
                                    };
                                }
                            }
                        }
                    } else {
                        // Normal mode handling
                        match key.code {
                            KeyCode::Char('R') => {
                                let tx = self.tx.clone();
                                let brew = self.brew.clone();
                                thread::spawn(move || match brew.outdated() {
                                    Ok(list) => {
                                        let _ = tx.send(AppEvent::OutdatedList(list));
                                    }
                                    Err(e) => {
                                        let _ = tx.send(AppEvent::Log(format!(
                                            "outdated check failed: {}",
                                            e
                                        )));
                                    }
                                });
                            }
                            KeyCode::Char('o') => {
                                let packages = self.outdated_items.clone();
                                let checked = vec![false; packages.len()];
                                self.mode = Mode::Outdated {
                                    packages,
                                    cursor: 0,
                                    checked,
                                    scroll: 0,
                                };
                            }
                            KeyCode::Char('q') => return Ok(()),
                            KeyCode::Char('?') => {
                                self.mode = Mode::Help;
                            }
                            KeyCode::Tab => {
                                self.focus = match self.focus {
                                    Focus::Installed => Focus::Available,
                                    Focus::Available => Focus::Installed,
                                };
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                if self.focus == Focus::Installed {
                                    if self.selected + 1 < self.items.len() {
                                        self.selected += 1;
                                    }
                                } else {
                                    if !self.available_filtered.is_empty() {
                                        if let Some(pos) = self
                                            .available_filtered
                                            .iter()
                                            .position(|&idx| idx == self.available_selected)
                                        {
                                            let next_pos =
                                                (pos + 1).min(self.available_filtered.len() - 1);
                                            self.available_selected =
                                                self.available_filtered[next_pos];
                                        } else {
                                            self.available_selected = self.available_filtered[0];
                                        }
                                    }
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                if self.focus == Focus::Installed {
                                    if self.selected > 0 {
                                        self.selected -= 1;
                                    }
                                } else {
                                    if !self.available_filtered.is_empty() {
                                        if let Some(pos) = self
                                            .available_filtered
                                            .iter()
                                            .position(|&idx| idx == self.available_selected)
                                        {
                                            let prev_pos = pos.saturating_sub(1);
                                            self.available_selected =
                                                self.available_filtered[prev_pos];
                                        } else {
                                            self.available_selected =
                                                *self.available_filtered.last().unwrap();
                                        }
                                    }
                                }
                            }
                            KeyCode::Char('r') => {
                                if let Some(f) = self.items.get(self.selected) {
                                    self.mode = Mode::Confirm {
                                        action: ConfirmAction::Uninstall,
                                        name: f.name.clone(),
                                        idx: Some(self.selected),
                                    };
                                }
                            }
                            KeyCode::Char('u') => {
                                if let Some(f) = self.items.get(self.selected) {
                                    self.mode = Mode::Confirm {
                                        action: ConfirmAction::Upgrade,
                                        name: f.name.clone(),
                                        idx: Some(self.selected),
                                    };
                                }
                            }
                            KeyCode::Char('i') => {
                                self.mode = Mode::Input {
                                    action: InputAction::Install,
                                    buffer: String::new(),
                                };
                            }
                            KeyCode::Char('s') => {
                                self.mode = Mode::Input {
                                    action: InputAction::Search,
                                    buffer: String::new(),
                                };
                            }
                            KeyCode::Char('f') => {
                                self.mode = Mode::Input {
                                    action: InputAction::Search,
                                    buffer: self.available_filter.clone(),
                                };
                                self.focus = Focus::Available;
                            }
                            KeyCode::Char('F') => {
                                self.available_filter.clear();
                                self.available_filtered = (0..self.available_items.len()).collect();
                            }
                            KeyCode::Enter => {
                                if self.focus == Focus::Installed {
                                    if let Some(f) = self.items.get(self.selected) {
                                        let name = f.name.clone();
                                        self.mode = Mode::Confirm {
                                            action: ConfirmAction::Uninstall,
                                            name: name.clone(),
                                            idx: Some(self.selected),
                                        };
                                        if self.last_selected
                                            != Some((Focus::Installed, self.selected))
                                        {
                                            let tx = self.tx.clone();
                                            let mut brew = self.brew.clone();
                                            let idx = self.selected;
                                            self.last_selected = Some((Focus::Installed, idx));
                                            thread::spawn(move || match brew.info(&name) {
                                                Ok(info) => {
                                                    let _ = tx.send(AppEvent::BrewInfo(info, idx));
                                                }
                                                Err(e) => {
                                                    let _ = tx.send(AppEvent::Log(format!(
                                                        "Info failed: {}",
                                                        e
                                                    )));
                                                }
                                            });
                                        }
                                    }
                                } else {
                                    if let Some(name) =
                                        self.available_items.get(self.available_selected)
                                    {
                                        let name = name.clone();
                                        self.mode = Mode::Confirm {
                                            action: ConfirmAction::Install,
                                            name: name.clone(),
                                            idx: Some(self.available_selected),
                                        };
                                        if self.last_selected
                                            != Some((Focus::Available, self.available_selected))
                                        {
                                            let tx = self.tx.clone();
                                            let mut brew = self.brew.clone();
                                            let idx = self.available_selected;
                                            self.last_selected = Some((Focus::Available, idx));
                                            thread::spawn(move || match brew.info(&name) {
                                                Ok(info) => {
                                                    let _ = tx.send(AppEvent::BrewInfoAvailable(
                                                        info, idx,
                                                    ));
                                                }
                                                Err(e) => {
                                                    let _ = tx.send(AppEvent::Log(format!(
                                                        "Info failed: {}",
                                                        e
                                                    )));
                                                }
                                            });
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            // automatic details loading
            match self.focus {
                Focus::Installed => {
                    if !self.items.is_empty() {
                        if self.last_selected != Some((Focus::Installed, self.selected)) {
                            let idx = self.selected;
                            if let Some(f) = self.items.get(idx) {
                                let name = f.name.clone();
                                let tx = self.tx.clone();
                                let mut brew = self.brew.clone();
                                self.last_selected = Some((Focus::Installed, idx));
                                thread::spawn(move || match brew.info(&name) {
                                    Ok(info) => {
                                        let _ = tx.send(AppEvent::BrewInfo(info, idx));
                                    }
                                    Err(e) => {
                                        let _ =
                                            tx.send(AppEvent::Log(format!("Info failed: {}", e)));
                                    }
                                });
                            }
                        }
                    }
                }
                Focus::Available => {
                    if !self.available_items.is_empty() {
                        if self.last_selected != Some((Focus::Available, self.available_selected)) {
                            let idx = self.available_selected;
                            if let Some(name) = self.available_items.get(idx) {
                                let name = name.clone();
                                let tx = self.tx.clone();
                                let mut brew = self.brew.clone();
                                self.last_selected = Some((Focus::Available, idx));
                                thread::spawn(move || match brew.info(&name) {
                                    Ok(info) => {
                                        let _ = tx.send(AppEvent::BrewInfoAvailable(info, idx));
                                    }
                                    Err(e) => {
                                        let _ =
                                            tx.send(AppEvent::Log(format!("Info failed: {}", e)));
                                    }
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}
