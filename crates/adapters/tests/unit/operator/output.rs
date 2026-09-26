use super::*;

#[test]
fn localized_human_failures_preserve_every_machine_byte_and_exit_status() {
    use darkhorse_domain::localization::Locale;
    for error in [
        Failure::usage(),
        Failure::confirmation(),
        Failure::interrupted(),
        Failure::output(),
        Failure::from("Administrator authentication or authority denied."),
        Failure::from("The principal changed; read its current revision before retrying."),
        Failure::from(
            "Outcome unknown; inspect authoritative audit and account state before retrying.",
        ),
    ] {
        assert_eq!(
            render_failure_in(&error, Format::Json, Locale::Spanish).unwrap(),
            render_failure(&error, Format::Json).unwrap()
        );
        assert_ne!(
            render_failure_in(&error, Format::Human, Locale::Spanish).unwrap(),
            render_failure(&error, Format::Human).unwrap()
        );
        assert_eq!(
            render_failure_in(&error, Format::Human, Locale::English).unwrap(),
            render_failure(&error, Format::Human).unwrap()
        );
    }
    let output = Output::record(serde_json::json!({"name":"literal-name", "revision":42}));
    assert_eq!(
        render_in(&output, Format::Json, Locale::Spanish).unwrap(),
        render(&output, Format::Json).unwrap()
    );
    assert!(
        String::from_utf8(render_in(&output, Format::Human, Locale::Spanish).unwrap())
            .unwrap()
            .starts_with("Resultado:")
    );
}

#[test]
fn translated_successes_preserve_identifiers_and_bound_the_complete_human_record() {
    use darkhorse_domain::localization::Locale;
    let id = "00000000-0000-0000-0000-000000000001";
    let output = Output::localized_message(
        format!("Completed: {id}"),
        format!("Completado: {id}"),
        serde_json::json!({"id":id}),
    );
    assert_eq!(
        render_in(&output, Format::Human, Locale::Spanish).unwrap(),
        format!("Completado: {id}\n").as_bytes()
    );
    assert_eq!(
        render_in(&output, Format::Json, Locale::Spanish).unwrap(),
        render(&output, Format::Json).unwrap()
    );
    let record = Output::record(serde_json::json!("a".repeat(OUTPUT_LIMIT - 3)));
    assert!(render(&record, Format::Human).is_ok());
    assert_eq!(
        render_in(&record, Format::Human, Locale::Spanish)
            .unwrap_err()
            .exit_code(),
        74
    );
    let text = Output::localized_message(
        "safe".into(),
        "literal\u{202e}é\n".into(),
        serde_json::Value::Null,
    );
    assert_eq!(
        render_in(&text, Format::Human, Locale::Spanish).unwrap(),
        "literal\\u202eé\\u000a\n".as_bytes()
    );
}
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
