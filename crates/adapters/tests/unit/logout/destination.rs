use super::*;
#[test]
fn destinations_require_canonical_https_without_credentials_or_fragments() {
    for value in [
        "https://app.example/logout",
        "https://app.example/logout?a=1",
        "https://[::1]:8443/logout",
    ] {
        assert_eq!(destination(value).unwrap().as_str(), value);
    }
    for value in [
        "http://app.example/logout",
        "https://u:p@app.example/logout",
        "https://app.example/#x",
        "https://app.example",
        "https://APP.example/logout",
        "https://app.example/\nlogout",
        "file:///etc/passwd",
    ] {
        assert!(destination(value).is_err(), "{value}");
    }
    assert!(destination(&format!("https://app.example/{}", "a".repeat(2048))).is_err());
}
