use homebrew_tui::brew::Brew;

#[test]
fn brew_search_print() {
    let b = Brew::new();
    match b.search("git") {
        Ok(results) => {
            println!("Search returned {} results", results.len());
            for r in results.iter().take(20) {
                println!("- {}", r);
            }
            assert!(!results.is_empty());
        }
        Err(e) => {
            panic!("brew.search failed: {}", e);
        }
    }
}
