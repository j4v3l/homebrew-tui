use serde_json::json;

#[test]
fn parse_brew_list_json() {
    let sample = json!({
        "formulae": [
            { "name": "openssl", "full_name": "openssl@1.1", "desc": "TLS/SSL toolkit" }
        ]
    });
    let s = sample.to_string();
    let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert!(parsed.get("formulae").is_some());
}
