use super::*;
use envbind::MapEnvironment;
use std::cell::Cell;
struct Fake {
    calls: Cell<usize>,
    bytes: Vec<u8>,
    failure: bool,
}
impl SecretFiles for Fake {
    fn read(&self, _: &str) -> Result<Vec<u8>, EnvironmentError> {
        self.calls.set(self.calls.get() + 1);
        if self.failure {
            Err(unavailable())
        } else {
            Ok(self.bytes.clone())
        }
    }
}
fn files(bytes: &[u8]) -> Fake {
    Fake {
        calls: Cell::new(0),
        bytes: bytes.into(),
        failure: false,
    }
}
#[test]
fn file_values_are_bounded_utf8_and_remove_only_one_terminal_newline() {
    let env =
        MapEnvironment::from_pairs([("DARKHORSE_DATABASE_URL_FILE", "/run/secrets/database")]);
    let fake = files(b" secret with spaces \r\n");
    assert_eq!(
        resolve(&env, &fake, "DARKHORSE_DATABASE_URL").unwrap(),
        Some(" secret with spaces ".into())
    );
    assert_eq!(fake.calls.get(), 1);
    for bytes in [
        vec![],
        vec![255],
        vec![b'x'; MAX_BYTES + 1],
        b"a\0b".to_vec(),
        b"\n".to_vec(),
        b"\r\n".to_vec(),
    ] {
        assert!(resolve(&env, &files(&bytes), "DARKHORSE_DATABASE_URL").is_err());
    }
    assert_eq!(decode(b"x\n\n".to_vec()).unwrap(), "x\n");
    assert_eq!(decode(vec![b'x'; MAX_BYTES]).unwrap().len(), MAX_BYTES);
}
#[test]
fn conflicts_invalid_paths_and_non_secret_settings_do_not_read_files() {
    let fake = files(b"secret");
    for pairs in [
        vec![
            ("DARKHORSE_DATABASE_URL", "direct"),
            ("DARKHORSE_DATABASE_URL_FILE", "/secret"),
        ],
        vec![("DARKHORSE_DATABASE_URL_FILE", "")],
        vec![("DARKHORSE_DATABASE_URL_FILE", "relative")],
        vec![("DARKHORSE_DATABASE_URL_FILE", "/bad\npath")],
    ] {
        assert!(
            resolve(
                &MapEnvironment::from_pairs(pairs),
                &fake,
                "DARKHORSE_DATABASE_URL"
            )
            .is_err()
        );
    }
    assert_eq!(
        resolve(
            &MapEnvironment::from_pairs([
                ("DARKHORSE_HTTP_PORT", "3001"),
                ("DARKHORSE_HTTP_PORT_FILE", "/secret")
            ]),
            &fake,
            "DARKHORSE_HTTP_PORT"
        )
        .unwrap(),
        Some("3001".into())
    );
    assert_eq!(fake.calls.get(), 0);
}
#[test]
fn absent_and_direct_values_keep_existing_binding_and_failures_are_redacted() {
    let fake = files(b"secret");
    assert_eq!(
        resolve(&MapEnvironment::new(), &fake, "DARKHORSE_DATABASE_URL").unwrap(),
        None
    );
    assert_eq!(
        resolve(
            &MapEnvironment::from_pairs([("DARKHORSE_DATABASE_URL", "direct")]),
            &fake,
            "DARKHORSE_DATABASE_URL"
        )
        .unwrap(),
        Some("direct".into())
    );
    let mut failure = files(b"");
    failure.failure = true;
    let error = resolve(
        &MapEnvironment::from_pairs([("DARKHORSE_DATABASE_URL_FILE", "/private-value")]),
        &failure,
        "DARKHORSE_DATABASE_URL",
    )
    .unwrap_err()
    .to_string();
    assert!(!error.contains("private-value"));
    assert_eq!(fake.calls.get(), 0);
}
