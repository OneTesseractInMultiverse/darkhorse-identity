use super::*;
#[test]
fn records_are_safe_for_a_terminal_and_still_valid_json() {
    let name = "a\x1b[31m\nforged\u{009b}b\u{202e}c\u{2028}d";
    let value = serde_json::json!({"name":name});
    let bytes = render(&Output::record(value.clone()), Format::Human).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert_eq!(text.lines().count(), 1);
    assert!(!text.contains('\x1b'));
    assert!(!text.contains('\u{009b}'));
    assert!(!text.contains('\u{202e}'));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).unwrap(),
        value
    );
    let json = render(&Output::record(value.clone()), Format::Json).unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(envelope["schema_version"], 1);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"], value);
}
#[test]
fn bounded_outputs_and_failed_writers_never_report_success() {
    assert!(
        render(
            &Output::record(serde_json::json!({"name":"x".repeat(OUTPUT_LIMIT)})),
            Format::Json
        )
        .is_err()
    );
    struct Broken;
    impl std::io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert_eq!(
        write_bytes(&mut Broken, b"public\n")
            .unwrap_err()
            .exit_code(),
        74
    );
    let mut bytes = Vec::new();
    write_bytes(&mut bytes, b"public\n").unwrap();
    assert_eq!(bytes, b"public\n");
    let error = Failure::usage();
    let json = render_failure(&error, Format::Json).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "invalid_arguments");
    assert_eq!(Failure::from("Operation failed.").exit_code(), 1);
}
