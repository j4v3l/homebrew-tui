use homebrew_tui::brew::Brew;

#[test]
fn brew_available_print() {
    let b = Brew::new();
    match b.all_available() {
        Ok(results) => {
            println!("Available returned {} results", results.len());
            for r in results.iter().take(30) {
                println!("- {}", r);
            }
            assert!(!results.is_empty());
        }
        Err(e) => {
            panic!("brew.all_available failed: {}", e);
        }
    }
}
