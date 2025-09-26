use anyhow::Result;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use homebrew_tui::app::App;
use homebrew_tui::brew::Brew;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::env;
use std::io;

fn main() -> Result<()> {
    // Debug helper: if HOMEBREW_TUI_DEBUG is set, print installed formulae and exit.
    if env::var("HOMEBREW_TUI_DEBUG").is_ok() {
        let brew = Brew::new();
        match brew.list_installed() {
            Ok(list) => {
                for f in list {
                    println!("{}", f.name);
                }
                return Ok(());
            }
            Err(e) => {
                eprintln!("Error listing installed formulae: {}", e);
                return Err(e);
            }
        }
    }
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new()?;
    let res = app.run(&mut terminal);

    // restore terminal
    disable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        eprintln!("Application error: {}", e);
        std::process::exit(1);
    }
    Ok(())
}
