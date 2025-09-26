// UI drawing for homebrew-tui
use crate::app::{App, Mode};
use anyhow::Result;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Span, Spans};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Terminal;
use std::io::Stdout;
use unicode_width::UnicodeWidthStr;

pub fn draw_ui(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    terminal.draw(|f| {
        let size = f.size();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(7)].as_ref())
            .split(size);

        // three-column layout: Installed | Available | Details
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Percentage(30),
                    Constraint::Percentage(35),
                    Constraint::Percentage(35),
                ]
                .as_ref(),
            )
            .split(chunks[0]);

        // list
        let items: Vec<ListItem> = app
            .items
            .iter()
            .map(|i| ListItem::new(Spans::from(vec![Span::raw(i.name.clone())])))
            .collect();
        let mut state = ListState::default();
        if !app.items.is_empty() {
            state.select(Some(app.selected));
        }
        // spinner frames
        let spinner_frames = ["◐", "◓", "◑", "◒", "◐", "◓", "◑", "◒"];
        let spin = spinner_frames[app.spinner_idx % spinner_frames.len()];

        let installed_title = if app.focus == crate::app::Focus::Installed {
            if app.loading_installed {
                format!("Installed (focused) {}", spin)
            } else {
                "Installed (focused)".to_string()
            }
        } else {
            if app.loading_installed {
                format!("Installed {}", spin)
            } else {
                "Installed".to_string()
            }
        };
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(installed_title),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );
        f.render_stateful_widget(list, main_chunks[0], &mut state);

        // available list (middle column)
        // build a quick set of installed names for marking
        let installed_names: std::collections::HashSet<String> =
            app.items.iter().map(|f| f.name.clone()).collect();

        // Render only filtered available items (show index mapping)
        let avail_items: Vec<ListItem> = app
            .available_filtered
            .iter()
            .filter_map(|&_idx| app.available_items.get(_idx).map(|name| (_idx, name)))
            .map(|(_idx, name)| {
                if installed_names.contains(name) {
                    ListItem::new(Spans::from(vec![Span::raw(format!(
                        "{} (Installed)",
                        name
                    ))]))
                } else {
                    ListItem::new(Spans::from(vec![Span::raw(name.clone())]))
                }
            })
            .collect();
        let mut avail_state = ListState::default();
        // find the position in the filtered list that corresponds to available_selected
        let filtered_sel = app
            .available_filtered
            .iter()
            .position(|&idx| idx == app.available_selected);
        if let Some(pos) = filtered_sel {
            avail_state.select(Some(pos));
        }
        let available_title = if app.focus == crate::app::Focus::Available {
            if app.loading_available {
                format!(
                    "Available ({}) (focused) {}",
                    app.available_items.len(),
                    spin
                )
            } else {
                format!("Available ({}) (focused)", app.available_items.len())
            }
        } else {
            if app.loading_available {
                format!("Available ({}) {}", app.available_items.len(), spin)
            } else {
                format!("Available ({})", app.available_items.len())
            }
        };
        let available_list = List::new(avail_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(available_title),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            );
        f.render_stateful_widget(available_list, main_chunks[1], &mut avail_state);

        // details (right column) — show full details for the currently-focused selection
        let detail = match app.focus {
            crate::app::Focus::Installed => {
                if let Some(sel) = app.items.get(app.selected) {
                    let mut lines = vec![];
                    lines.push(Spans::from(Span::raw(format!("{}", sel.name))));
                    if let Some(fn_) = &sel.full_name {
                        lines.push(Spans::from(Span::raw(format!("full: {}", fn_))));
                    }
                    if let Some(desc) = &sel.desc {
                        lines.push(Spans::from(Span::raw(""))); // spacer
                        lines.push(Spans::from(Span::raw(desc.clone())));
                    }
                    if let Some(h) = &sel.homepage {
                        lines.push(Spans::from(Span::raw(format!("homepage: {}", h))));
                    }
                    if let Some(l) = &sel.license {
                        lines.push(Spans::from(Span::raw(format!("license: {}", l))));
                    }
                    if !sel.dependencies.is_empty() {
                        lines.push(Spans::from(Span::raw("")));
                        lines.push(Spans::from(Span::raw("dependencies:")));
                        for d in sel.dependencies.iter() {
                            lines.push(Spans::from(Span::raw(format!("  - {}", d))));
                        }
                    }
                    if !sel.installed.is_empty() {
                        lines.push(Spans::from(Span::raw("")));
                        lines.push(Spans::from(Span::raw("installed:")));
                        for inst in sel.installed.iter() {
                            lines.push(Spans::from(Span::raw(format!("  - {}", inst.version))));
                        }
                    }
                    if let Some(c) = &sel.caveats {
                        if !c.trim().is_empty() {
                            lines.push(Spans::from(Span::raw("")));
                            lines.push(Spans::from(Span::raw("caveats:")));
                            for l in c.lines() {
                                lines.push(Spans::from(Span::raw(format!("  {}", l))));
                            }
                        }
                    }

                    Paragraph::new(lines)
                        .block(Block::default().borders(Borders::ALL).title("Details"))
                        .wrap(Wrap { trim: false })
                } else {
                    Paragraph::new("No package selected")
                        .block(Block::default().borders(Borders::ALL).title("Details"))
                }
            }
            crate::app::Focus::Available => {
                if let Some(details) = &app.available_details {
                    let mut lines = vec![];
                    lines.push(Spans::from(Span::raw(format!("{}", details.name))));
                    if let Some(fn_) = &details.full_name {
                        lines.push(Spans::from(Span::raw(format!("full: {}", fn_))));
                    }
                    if let Some(desc) = &details.desc {
                        lines.push(Spans::from(Span::raw("")));
                        lines.push(Spans::from(Span::raw(desc.clone())));
                    }
                    if let Some(h) = &details.homepage {
                        lines.push(Spans::from(Span::raw(format!("homepage: {}", h))));
                    }
                    if let Some(l) = &details.license {
                        lines.push(Spans::from(Span::raw(format!("license: {}", l))));
                    }
                    if !details.dependencies.is_empty() {
                        lines.push(Spans::from(Span::raw("")));
                        lines.push(Spans::from(Span::raw("dependencies:")));
                        for d in details.dependencies.iter() {
                            lines.push(Spans::from(Span::raw(format!("  - {}", d))));
                        }
                    }
                    if !details.installed.is_empty() {
                        lines.push(Spans::from(Span::raw("")));
                        lines.push(Spans::from(Span::raw("installed:")));
                        for inst in details.installed.iter() {
                            lines.push(Spans::from(Span::raw(format!("  - {}", inst.version))));
                        }
                    }
                    if let Some(c) = &details.caveats {
                        if !c.trim().is_empty() {
                            lines.push(Spans::from(Span::raw("")));
                            lines.push(Spans::from(Span::raw("caveats:")));
                            for l in c.lines() {
                                lines.push(Spans::from(Span::raw(format!("  {}", l))));
                            }
                        }
                    }

                    Paragraph::new(lines)
                        .block(Block::default().borders(Borders::ALL).title("Details"))
                        .wrap(Wrap { trim: false })
                } else if let Some(name) = app.available_items.get(app.available_selected) {
                    Paragraph::new(format!("{}\n\n{}", name, "(no details loaded)"))
                        .block(Block::default().borders(Borders::ALL).title("Details"))
                } else {
                    Paragraph::new("No package selected")
                        .block(Block::default().borders(Borders::ALL).title("Details"))
                }
            }
        };
        f.render_widget(detail, main_chunks[2]);

        // bottom: logs and status
        let bottom_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)].as_ref())
            .split(chunks[1]);

        // logs
        let logs: Vec<ListItem> = app
            .logs
            .iter()
            .rev()
            .take(100)
            .map(|l| ListItem::new(Span::raw(l.clone())))
            .collect();
        let logs_block = List::new(logs).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Logs (recent)"),
        );
        f.render_widget(logs_block, bottom_chunks[0]);

        // status
        // Operation progress (gauge when percent available) above status
        let right_bottom = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)].as_ref())
            .split(bottom_chunks[1]);

        if let Some(pct) = app.operation_percent {
            // render a Gauge with animated label
            use ratatui::widgets::Gauge;
            let ratio = (pct as f64) / 100.0;
            let label = if app.operating {
                format!(
                    "{}% {}",
                    pct,
                    spinner_frames[app.spinner_idx % spinner_frames.len()]
                )
            } else {
                format!("{}%", pct)
            };
            let gauge = Gauge::default()
                .block(Block::default().borders(Borders::ALL).title("Op Progress"))
                .gauge_style(Style::default().fg(Color::Green).bg(Color::Black))
                .label(label)
                .ratio(ratio);
            f.render_widget(gauge, right_bottom[0]);
        } else {
            let mut op_progress = app
                .operation_status
                .clone()
                .unwrap_or_else(|| "".to_string());
            if app.operating {
                op_progress = format!(
                    "{} {}",
                    spinner_frames[app.spinner_idx % spinner_frames.len()],
                    op_progress
                );
            }
            let op_paragraph = Paragraph::new(Spans::from(vec![Span::raw(op_progress)]))
                .block(Block::default().borders(Borders::ALL).title("Op Progress"))
                .alignment(Alignment::Left);
            f.render_widget(op_paragraph, right_bottom[0]);
        }

        // Status: render multi-line status so fields wrap and are readable
        let mut status_lines: Vec<Spans> = vec![];
        status_lines.push(Spans::from(Span::raw(format!(
            "Installed: {}  Available: {}",
            app.items.len(),
            app.available_items.len()
        ))));
        let focus_str = if app.focus == crate::app::Focus::Installed {
            "Installed"
        } else {
            "Available"
        };
        status_lines.push(Spans::from(Span::raw(format!("Focus: {}", focus_str))));

        // show if there are updates/upgrades available for installed packages
        let updates_count = app.outdated_items.len();
        if updates_count > 0 {
            // show a short preview (first 5 names) to avoid overflowing the status pane
            let preview: Vec<String> = app.outdated_items.iter().take(5).cloned().collect();
            let preview_str = if preview.is_empty() {
                "".to_string()
            } else if preview.len() == 5 {
                format!("{}...", preview.join(", "))
            } else {
                preview.join(", ")
            };
            status_lines.push(Spans::from(Span::styled(
                format!("Updates available: {} {}", updates_count, preview_str),
                Style::default().fg(Color::Yellow),
            )));
        } else {
            status_lines.push(Spans::from(Span::raw("Updates available: 0")));
        }

        // mode and logs
        let mode_str = match &app.mode {
            Mode::Normal => "Normal".to_string(),
            Mode::Help => "Help".to_string(),
            Mode::Input { action, .. } => match action {
                crate::app::InputAction::Install => "Input(Install)".to_string(),
                crate::app::InputAction::Search => "Input(Search)".to_string(),
            },
            Mode::Confirm { action, name, .. } => match action {
                crate::app::ConfirmAction::Install => format!("Confirm Install {}", name),
                crate::app::ConfirmAction::Uninstall => format!("Confirm Uninstall {}", name),
                crate::app::ConfirmAction::Upgrade => format!("Confirm Upgrade {}", name),
                crate::app::ConfirmAction::BulkUpgrade(_) => {
                    format!("Confirm Bulk Upgrade {}", name)
                }
                crate::app::ConfirmAction::InstallBrew => format!("Confirm Install Homebrew"),
            },
            Mode::SearchResults { results, selected } => {
                format!("SearchResults {} results (sel {})", results.len(), selected)
            }
            Mode::Outdated {
                packages, cursor, ..
            } => {
                format!("Outdated {} packages (sel {})", packages.len(), cursor)
            }
            Mode::Operation { title, logs, .. } => {
                format!("Operation: {} ({} lines)", title, logs.len())
            }
        };
        status_lines.push(Spans::from(Span::raw(format!(
            "Mode: {}  Logs: {}",
            mode_str,
            app.logs.len()
        ))));

        if let Some(ts) = app.last_refreshed {
            if let Ok(dur) = ts.elapsed() {
                let secs = dur.as_secs();
                status_lines.push(Spans::from(Span::raw(format!("refreshed {}s ago", secs))));
            }
        }

        let status = Paragraph::new(status_lines)
            .block(Block::default().borders(Borders::ALL).title("Status"))
            .wrap(Wrap { trim: true });

        f.render_widget(status, right_bottom[1]);

        // Overlays: input prompt or confirmation modal
        match &app.mode {
            Mode::SearchResults { results, selected } => {
                // Calculate dynamic size: width based on longest result, height based on number of results
                let max_cols = size.width.max(1) as usize;
                let max_rows = size.height.max(1) as usize;

                // longest string display width (unicode-aware)
                let longest = results
                    .iter()
                    .map(|s| UnicodeWidthStr::width(s.as_str()))
                    .max()
                    .unwrap_or(10);
                // desired width with padding
                let desired_w = (longest + 8) as u16; // padding for borders and margin
                                                      // clamp desired width to terminal width minus a small margin
                let clamped_w = desired_w.min((max_cols.saturating_sub(6)) as u16).max(20);

                // desired height: one row per result up to a max (keep some room)
                let desired_rows = (results.len())
                    .min((max_rows.saturating_sub(6)) as usize)
                    .max(1);
                let desired_h = (desired_rows as u16) + 4; // padding for title and borders
                let clamped_h = desired_h.min((max_rows.saturating_sub(4)) as u16).max(4);

                // convert to percent for centered_rect helper
                let percent_x = ((clamped_w as f32 / size.width as f32) * 100.0).round() as u16;
                let percent_y = ((clamped_h as f32 / size.height as f32) * 100.0).round() as u16;

                // ensure percent clamped within reasonable bounds (allow smaller min to avoid large empty space)
                let percent_x = percent_x.clamp(15, 95);
                let percent_y = percent_y.clamp(5, 95);

                // Build items and support internal scrolling if too many results
                let title = "Search Results";
                // compute how many rows we can show inside modal body
                let body_height = (clamped_h.saturating_sub(4)) as usize; // account for borders/title
                let total = results.len();

                // determine visible window start based on selected
                let sel = *selected;
                let start = if sel >= body_height && total > body_height {
                    sel.saturating_sub(body_height / 2)
                        .min(total.saturating_sub(body_height))
                } else {
                    0
                };
                let end = (start + body_height).min(total);

                let visible: Vec<ListItem> = results[start..end]
                    .iter()
                    .map(|r| ListItem::new(Spans::from(vec![Span::raw(r.clone())])))
                    .collect();

                let area = centered_rect(percent_x, percent_y, size);
                let mut state = ListState::default();
                if !visible.is_empty() {
                    // map selected into visible range
                    let vis_sel = if sel >= start && sel < end {
                        sel - start
                    } else {
                        0
                    };
                    state.select(Some(vis_sel));
                }
                let list = List::new(visible)
                    .block(Block::default().borders(Borders::ALL).title(title))
                    .highlight_style(
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    );
                f.render_widget(Clear, area);
                f.render_stateful_widget(list, area, &mut state);

                // if not all items are visible, show a small footer indicator
                if end < total {
                    let footer_area = Rect {
                        x: area.x,
                        y: area.y + area.height - 1,
                        width: area.width,
                        height: 1,
                    };
                    let footer = Paragraph::new(Spans::from(vec![Span::raw(format!(
                        "Showing {}..{} of {} (↑/↓ to move)",
                        start + 1,
                        end,
                        total
                    ))]))
                    .alignment(Alignment::Center);
                    f.render_widget(footer, footer_area);
                }
            }
            Mode::Help => {
                let area = centered_rect(70, 60, size);
                let title = "Help";
                let help_text = vec![
                    Spans::from(Span::raw("Keys:")),
                    Spans::from(Span::raw("  Navigation:")),
                    Spans::from(Span::raw("    j / Down    - move down")),
                    Spans::from(Span::raw("    k / Up      - move up")),
                    Spans::from(Span::raw("    Tab         - switch focus between Installed/Available")),
                    Spans::from(Span::raw("")),
                    Spans::from(Span::raw("  Actions:")),
                    Spans::from(Span::raw("    Enter       - open details / confirm action when applicable")),
                    Spans::from(Span::raw("    i           - install (opens input prompt)")),
                    Spans::from(Span::raw("    s           - search (opens input prompt)")),
                    Spans::from(Span::raw("    f           - focus Available and prefill search with current filter")),
                    Spans::from(Span::raw("    F           - clear Available filter")),
                    Spans::from(Span::raw("    r           - uninstall selected installed package (confirm)")),
                    Spans::from(Span::raw("    u           - upgrade selected installed package (confirm)")),
                    Spans::from(Span::raw("    o           - open Outdated packages modal")),
                    Spans::from(Span::raw("    R           - refresh outdated check (background)")),
                    Spans::from(Span::raw("    q           - quit")),
                    Spans::from(Span::raw("")),
                    Spans::from(Span::raw("  Outdated modal:")),
                    Spans::from(Span::raw("    ↑ / ↓ / j / k - move, Space: toggle package selection")),
                    Spans::from(Span::raw("    Enter         - confirm selected upgrades (bulk)")),
                    Spans::from(Span::raw("    Esc           - close Outdated modal")),
                    Spans::from(Span::raw("")),
                    Spans::from(Span::raw("  Confirm dialogs:")),
                    Spans::from(Span::raw("    y / Enter     - confirm the action")),
                    Spans::from(Span::raw("    n / Esc       - cancel")),
                    Spans::from(Span::raw("")),
                    Spans::from(Span::raw("  Operation modal (logs):")),
                    Spans::from(Span::raw("    ↑ / ↓ / j / k - scroll lines")),
                    Spans::from(Span::raw("    PgUp / PgDn   - page up / page down")),
                    Spans::from(Span::raw("    Home / End    - jump to top / bottom (most recent)")),
                    Spans::from(Span::raw("    Esc / ?       - close Operation modal")),
                    Spans::from(Span::raw("")),
                    Spans::from(Span::raw("Press ? or Esc to close")),
                ];
                let paragraph = Paragraph::new(help_text)
                    .block(Block::default().borders(Borders::ALL).title(title))
                    .alignment(Alignment::Left);
                f.render_widget(Clear, area);
                f.render_widget(paragraph, area);
            }
            Mode::Outdated {
                packages,
                cursor,
                checked,
                scroll: _,
            } => {
                let area = centered_rect(60, 50, size);
                let title = format!("Outdated packages ({} updates)", packages.len());
                let mut items: Vec<ListItem> = vec![];
                for (i, p) in packages.iter().enumerate() {
                    let mark = if checked.get(i).copied().unwrap_or(false) {
                        "[x]"
                    } else {
                        "[ ]"
                    };
                    items.push(ListItem::new(Spans::from(vec![Span::raw(format!(
                        "{} {}",
                        mark, p
                    ))])));
                }
                let mut state = ListState::default();
                if !packages.is_empty() {
                    state.select(Some(*cursor));
                }
                let list = List::new(items)
                    .block(Block::default().borders(Borders::ALL).title(title))
                    .highlight_style(
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    );
                f.render_widget(Clear, area);
                f.render_stateful_widget(list, area, &mut state);
                // footer instructions
                let footer_area = Rect {
                    x: area.x,
                    y: area.y + area.height - 1,
                    width: area.width,
                    height: 1,
                };
                let footer = Paragraph::new(Spans::from(vec![Span::raw(
                    "Space: toggle  Enter: confirm  Esc: close",
                )]))
                .alignment(Alignment::Center);
                f.render_widget(footer, footer_area);
            }

            Mode::Confirm { action, name, idx } => {
                // use same size as SearchResults for visual consistency
                let area = centered_rect(60, 40, size);
                let title = match action {
                    crate::app::ConfirmAction::Uninstall => "Confirm Uninstall",
                    crate::app::ConfirmAction::Upgrade => "Confirm Upgrade",
                    crate::app::ConfirmAction::Install => "Confirm Install",
                    crate::app::ConfirmAction::BulkUpgrade(_) => "Confirm Bulk Upgrade",
                    crate::app::ConfirmAction::InstallBrew => "Confirm Install Homebrew",
                };

                // If idx provided, try to render richer details
                let paragraph = if let Some(i) = idx {
                    // prefer installed details if index matches items
                    if let Some(pkg) = app.items.get(*i) {
                        // build detail lines similar to Details pane
                        let mut lines: Vec<Spans> = vec![];
                        lines.push(Spans::from(Span::raw(format!("{}", pkg.name))));
                        if let Some(fn_) = &pkg.full_name {
                            lines.push(Spans::from(Span::raw(format!("full: {}", fn_))));
                        }
                        if let Some(desc) = &pkg.desc {
                            lines.push(Spans::from(Span::raw("")));
                            lines.push(Spans::from(Span::raw(desc.clone())));
                        }
                        lines.push(Spans::from(Span::raw("")));
                        lines.push(Spans::from(Span::raw(format!(
                            "{} '{}' ? (y/N)",
                            title, name
                        ))));
                        Paragraph::new(lines)
                            .block(Block::default().borders(Borders::ALL).title(title))
                            .wrap(Wrap { trim: false })
                    } else if let Some(details) = &app.available_details {
                        // available details (if loaded)
                        let mut lines: Vec<Spans> = vec![];
                        lines.push(Spans::from(Span::raw(format!("{}", details.name))));
                        if let Some(desc) = &details.desc {
                            lines.push(Spans::from(Span::raw("")));
                            lines.push(Spans::from(Span::raw(desc.clone())));
                        }
                        lines.push(Spans::from(Span::raw("")));
                        lines.push(Spans::from(Span::raw(format!(
                            "{} '{}' ? (y/N)",
                            title, name
                        ))));
                        Paragraph::new(lines)
                            .block(Block::default().borders(Borders::ALL).title(title))
                            .wrap(Wrap { trim: false })
                    } else {
                        // Build a small set of lines for the generic confirm dialog. If this is the
                        // InstallBrew action, add a short explanatory help text warning about
                        // possible sudo prompts and network access.
                        let mut lines: Vec<Spans> = vec![
                            Spans::from(Span::raw(format!("{} '{}' ? (y/N)", title, name))),
                            Spans::from(Span::raw("")),
                        ];
                        if let crate::app::ConfirmAction::InstallBrew = action {
                            lines.push(Spans::from(Span::raw(
                                "This will run the official Homebrew installer script from https://brew.sh.",
                            )));
                            lines.push(Spans::from(Span::raw(
                                "The installer may prompt for sudo and other interactive input and requires network access.",
                            )));
                            lines.push(Spans::from(Span::raw(
                                "If you prefer, cancel and run the installer manually in a terminal.",
                            )));
                            lines.push(Spans::from(Span::raw("")));
                        }
                        lines.push(Spans::from(Span::raw("Press Y to confirm, N or Esc to cancel.")));
                        Paragraph::new(lines)
                            .block(Block::default().borders(Borders::ALL).title(title))
                            .wrap(Wrap { trim: false })
                    }
                } else {
                    Paragraph::new(format!("{} '{}' ? (y/N)", title, name))
                        .block(Block::default().borders(Borders::ALL).title(title))
                };

                f.render_widget(Clear, area); // clear underlying
                f.render_widget(paragraph, area);
            }
            Mode::Operation {
                title,
                logs,
                scroll,
            } => {
                let area = centered_rect(60, 40, size);
                let block = Block::default().borders(Borders::ALL).title(title.as_str());
                // logs are chronological (oldest first). `scroll` is number of lines scrolled up from bottom.
                let total = logs.len();
                let height = (area.height.saturating_sub(2)) as usize; // leave space for borders
                                                                       // determine window: show newest lines up to height, offset by scroll
                let start_idx = if total > height + *scroll {
                    total - height - *scroll
                } else {
                    0
                };
                let end_idx = start_idx + height.min(total.saturating_sub(start_idx));
                let text: Vec<Spans> = logs[start_idx..end_idx]
                    .iter()
                    .map(|l| Spans::from(Span::raw(l.clone())))
                    .collect();
                let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
                f.render_widget(Clear, area);
                f.render_widget(paragraph, area);
                // footer with simple position info
                let footer = Paragraph::new(Spans::from(vec![Span::raw(format!(
                    "lines {}/{} (↑/↓ scroll, PgUp/PgDn, Home/End)",
                    end_idx, total
                ))]))
                .alignment(Alignment::Right);
                let footer_area = Rect {
                    x: area.x,
                    y: area.y + area.height - 1,
                    width: area.width,
                    height: 1,
                };
                f.render_widget(footer, footer_area);
            }
            Mode::Input { action, buffer } => {
                // render a small, single-line input (like a password/short text field)
                let area = centered_rect(40, 10, size);
                let title = match action {
                    crate::app::InputAction::Install => "Install package",
                    crate::app::InputAction::Search => "Search packages",
                };
                let text = buffer.clone();
                // display the buffer inline
                let paragraph = Paragraph::new(text)
                    .block(Block::default().borders(Borders::ALL).title(title))
                    .alignment(Alignment::Center);
                f.render_widget(Clear, area);
                f.render_widget(paragraph, area);
            }
            _ => {}
        }
    })?;
    Ok(())
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    let middle = popup_layout[1];

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(middle);

    horizontal[1]
}
