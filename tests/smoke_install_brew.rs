use homebrew_tui::app::{App, AppEvent};

// Smoke test: simulate installer events without running any external process.
#[test]
fn smoke_install_brew_events() {
    // Construct app (this will spawn background threads which are safe for the test)
    let mut app = App::new().expect("app init");

    // Simulate start of an install-homebrew operation
    let title = "install-homebrew".to_string();
    // Before starting, ensure not operating
    assert!(!app.operating);

    // Send OpStart
    app.handle_event(AppEvent::OpStart(title.clone()));
    // App should now be in Operation mode and operating true
    assert!(app.operating);
    match &app.mode {
        homebrew_tui::app::Mode::Operation { title: t, logs, .. } => {
            assert_eq!(t, &title);
            assert!(logs.is_empty());
        }
        _ => panic!("expected Operation mode"),
    }

    // Simulate streaming logs
    app.handle_event(AppEvent::OpLog("Downloading... 10%".into()));
    app.handle_event(AppEvent::OpLog("Downloading... 50%".into()));
    app.handle_event(AppEvent::OpLog("Installing... 100%".into()));

    // Check that operation_percent updated
    assert_eq!(app.operation_percent, Some(100));
    // Check that logs contain entries
    assert!(app.logs.iter().any(|l| l.contains("Downloading...")));

    // End operation
    app.handle_event(AppEvent::OpEnd(title.clone()));
    assert!(!app.operating);
    assert!(app.operation_percent.is_none());
}
