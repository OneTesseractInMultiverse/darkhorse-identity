use super::*;

#[test]
fn declarations_preserve_methods_handlers_and_feature_variants() {
    let source = r#"
    fn router() { Router::new().route("/token", post(redeem::<S>))
      .route("/api/profile", get(read::<S>).post(update::<S>))
      .route_service("/console", ServeFile::new(file))
      .nest_service("/_app", ServeDir::new(directory)); }
    fn disabled() { Router::new().route("/token", post(unavailable)); }
    "#;
    let entries = parse("src/router.rs", source).unwrap();
    assert_eq!(entries.len(), 6);
    assert!(entries.iter().any(|e| e.path == "/token"
        && e.method == "POST"
        && e.handler == "redeem"
        && e.function == "router"));
    assert!(
        entries
            .iter()
            .any(|e| e.path == "/token" && e.handler == "unavailable" && e.function == "disabled")
    );
    assert!(
        entries
            .iter()
            .any(|e| e.path == "/api/profile" && e.method == "GET" && e.handler == "read")
    );
    assert!(
        entries
            .iter()
            .any(|e| e.path == "/api/profile" && e.method == "POST" && e.handler == "update")
    );
    assert!(
        entries
            .iter()
            .any(|e| e.path == "/console" && e.method == "SERVICE")
    );
    assert!(
        entries
            .iter()
            .any(|e| e.path == "/_app" && e.method == "SERVICE_PREFIX")
    );
    assert_eq!(entries, parse("src/router.rs", source).unwrap());
}

#[test]
fn comments_and_string_examples_do_not_create_operations_and_unknown_syntax_fails() {
    assert!(parse("src/x.rs", r##"fn example() { let x = r#".route("/fake", post(fake))"#; /* .route("/fake", post(fake)) */ }"##).unwrap().is_empty());
    for source in [
        "fn broken(",
        "fn router() { Router::new().route(PATH, get(handler)); }",
        "fn router() { Router::new().route(\"/path\", undocumented()); }",
        "fn router() { Router::new().route(\"/path\", get(|_| async { 200 })); }",
        "fn router() { Router::new().nest(\"/prefix\", routes); }",
    ] {
        assert!(
            parse("src/x.rs", source).is_err(),
            "unknown route construction requires explicit support"
        );
    }
    assert!(parse("../outside.rs", "fn sample() {}").is_err());
    assert!(parse("src/x.rs", &" ".repeat(1_048_577)).is_err());
}

#[test]
fn changed_routes_cannot_silently_match_a_previous_inventory() {
    let before = parse(
        "src/x.rs",
        "fn router() { Router::new().route(\"/a\", get(read)); }",
    )
    .unwrap();
    let after = parse(
        "src/x.rs",
        "fn router() { Router::new().route(\"/a\", post(write)); }",
    )
    .unwrap();
    assert_ne!(before, after);
    let mut duplicate = before.clone();
    duplicate.extend(before.clone());
    assert!(merge(duplicate).is_err());
    let mut changed = before;
    changed.extend(after);
    assert_eq!(merge(changed).unwrap().len(), 2);
}

#[test]
fn aggregate_source_bytes_are_bounded_without_integer_overflow() {
    assert_eq!(bounded_total(8, 2, 10), Ok(10));
    assert_eq!(bounded_total(10, 0, 10), Ok(10));
    assert_eq!(bounded_total(10, 1, 10), Err(Error::Limit));
    assert_eq!(bounded_total(usize::MAX, 1, usize::MAX), Err(Error::Limit));
}

#[test]
fn route_inventory_entry_bound_fails_closed() {
    let mut source = String::from("fn router() {");
    for index in 0..=MAX_ENTRIES {
        source.push_str(&format!(
            "Router::new().route(\"/route-{index}\", get(handler));"
        ));
    }
    source.push('}');

    assert_eq!(parse("src/router.rs", &source), Err(Error::Limit));
}
