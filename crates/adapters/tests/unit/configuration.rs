use super::*;
use envbind::{EnvironmentError, MapEnvironment};

#[test]
fn defaults_are_validated_without_files_or_process_environment() {
    let settings = load(MapEnvironment::new()).unwrap();
    assert_eq!(settings.listen, "127.0.0.1:3001".parse().unwrap());
    assert_eq!(settings.public_origin.as_str(), "https://localhost:8443/");
    assert_eq!(settings.static_dir, PathBuf::from("apps/console/build"));
}

#[test]
fn accepts_explicit_ipv6_and_https_origin() {
    let settings = load(MapEnvironment::from_pairs([
        ("DARKHORSE_HTTP_HOST", "::1"),
        ("DARKHORSE_HTTP_PORT", "3101"),
        ("DARKHORSE_PUBLIC_ORIGIN", "https://identity.example.org"),
        ("DARKHORSE_STATIC_DIR", "/srv/console"),
    ]))
    .unwrap();
    assert_eq!(settings.listen, "[::1]:3101".parse().unwrap());
    assert_eq!(
        settings.public_origin.as_str(),
        "https://identity.example.org/"
    );
    assert_eq!(settings.static_dir, PathBuf::from("/srv/console"));
}

#[test]
fn rejects_invalid_listeners_without_disclosing_raw_input() {
    for (name, value) in [
        ("DARKHORSE_HTTP_HOST", "private-invalid-host"),
        ("DARKHORSE_HTTP_PORT", "0"),
        ("DARKHORSE_HTTP_PORT", "65536"),
        ("DARKHORSE_HTTP_PORT", "secret-invalid-port"),
    ] {
        let error = load(MapEnvironment::from_pairs([(name, value)])).unwrap_err();
        assert!(!format!("{error:?}").contains(value));
    }
}

#[test]
fn rejects_noncanonical_origins() {
    for value in [
        "not-a-url",
        "http://localhost",
        "https://user:secret@example.org",
        "https://example.org/path",
        "https://example.org/?secret=yes",
        "https://example.org/#fragment",
        "https://example.org:0",
    ] {
        assert_eq!(
            load(MapEnvironment::from_pairs([(
                "DARKHORSE_PUBLIC_ORIGIN",
                value
            )]))
            .unwrap_err(),
            ConfigurationError::PublicOrigin
        );
    }
}

#[test]
fn rejects_blank_and_oversized_paths() {
    assert_eq!(
        load(MapEnvironment::from_pairs([("DARKHORSE_STATIC_DIR", "  ")])).unwrap_err(),
        ConfigurationError::StaticDirectory
    );
    assert_eq!(
        load(MapEnvironment::from_pairs([(
            "DARKHORSE_STATIC_DIR",
            "x".repeat(4097)
        )]))
        .unwrap_err(),
        ConfigurationError::Binding
    );
}

struct BrokenEnvironment;

impl Environment for BrokenEnvironment {
    fn get(&self, _name: &str) -> Result<Option<String>, EnvironmentError> {
        Err(EnvironmentError::not_unicode())
    }
}

#[test]
fn reports_environment_failure_without_ambient_dependencies() {
    assert_eq!(
        load(BrokenEnvironment).unwrap_err(),
        ConfigurationError::Binding
    );
}

#[test]
fn rejects_oversized_public_origin_at_the_binding_boundary() {
    let input = "https://".to_owned() + &"x".repeat(2048);
    assert_eq!(
        load(MapEnvironment::from_pairs([(
            "DARKHORSE_PUBLIC_ORIGIN",
            input
        )]))
        .unwrap_err(),
        ConfigurationError::Binding
    );
}

#[test]
fn rejects_oversized_host_at_the_binding_boundary() {
    assert_eq!(
        load(MapEnvironment::from_pairs([(
            "DARKHORSE_HTTP_HOST",
            "x".repeat(129)
        )]))
        .unwrap_err(),
        ConfigurationError::Binding
    );
}
