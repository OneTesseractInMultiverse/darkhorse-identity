use super::*;
use envbind::MapEnvironment;
fn settings(extra: &[(&str, &str)]) -> MapEnvironment {
    let mut map = std::collections::BTreeMap::from([
        ("DARKHORSE_EMAIL_ENABLED", "true"),
        (
            "DARKHORSE_EMAIL_KEY",
            "abababababababababababababababababababababababababababababababab",
        ),
        ("DARKHORSE_SMTP_HOST", "smtp.example.com"),
        ("DARKHORSE_SMTP_FROM", "identity@example.com"),
    ]);
    map.extend(extra.iter().copied());
    MapEnvironment::from_pairs(map)
}
#[test]
fn smtp_is_opt_in_with_explicit_key_address_tls_host_and_paired_credentials() {
    assert!(load(MapEnvironment::new()).unwrap().is_none());
    assert_eq!(load(settings(&[])).unwrap().unwrap().port, 465);
    let configured = load(settings(&[
        ("DARKHORSE_SMTP_USERNAME", "name"),
        ("DARKHORSE_SMTP_PASSWORD", "secret"),
        ("DARKHORSE_SMTP_CA_FILE", "/private/root.pem"),
        ("DARKHORSE_SMTP_PORT", "2465"),
    ]))
    .unwrap()
    .unwrap();
    assert_eq!(configured.port, 2465);
    assert!(configured.ca_file.is_some());
    for (key, value) in [
        ("DARKHORSE_EMAIL_ENABLED", "bad"),
        ("DARKHORSE_EMAIL_KEY", "bad"),
        ("DARKHORSE_SMTP_PORT", "0"),
        ("DARKHORSE_SMTP_HOST", "smtp://example.com"),
        ("DARKHORSE_SMTP_HOST", "-bad.example"),
        ("DARKHORSE_SMTP_FROM", "bad"),
        ("DARKHORSE_SMTP_FROM", "a@example.com\r\nBcc: b@example.com"),
        ("DARKHORSE_SMTP_USERNAME", "name"),
    ] {
        assert!(load(settings(&[(key, value)])).is_err(), "{key}");
    }
}
