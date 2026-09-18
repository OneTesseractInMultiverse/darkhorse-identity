use super::*;
fn app(n: u128) -> ApplicationId {
    ApplicationId::from_u128(n).unwrap()
}
fn resource(n: u128) -> ResourceId {
    ResourceId::from_u128(n).unwrap()
}
fn scope(n: u128) -> ScopeId {
    ScopeId::from_u128(n).unwrap()
}
fn redirects() -> Redirects {
    Redirects::from_validated_urls(vec!["https://client.example/cb?x=1".into()]).unwrap()
}
fn spec() -> ClientSpec {
    ClientSpec::new(
        Label::new(" Client ").unwrap(),
        true,
        redirects(),
        vec![resource(1)],
        vec![scope(1)],
        "client_secret_basic",
    )
    .unwrap()
}
#[test]
fn bounds_labels_scope_tokens_methods_and_duplicate_grants() {
    assert_eq!(Label::new(" App ").unwrap().as_str(), "App");
    assert_eq!(
        ScopeName::new("orders:read").unwrap().as_str(),
        "orders:read"
    );
    for value in ["".into(), "\n".into(), "a".repeat(101), "a\nb".into()] {
        assert!(Label::new(&value).is_err());
    }
    for value in [
        "",
        "orders read",
        "x\"y",
        "x\\y",
        "é",
        "openid",
        "profile",
        "offline_access",
    ] {
        assert!(ScopeName::new(value).is_err());
    }
    for method in [
        "none",
        "client_secret_post",
        "client_secret_jwt",
        "private_key_jwt",
        "",
    ] {
        assert!(
            ClientSpec::new(
                Label::new("Client").unwrap(),
                true,
                redirects(),
                vec![],
                vec![],
                method
            )
            .is_err()
        );
    }
    for (resources, scopes) in [
        (vec![resource(1); 2], vec![]),
        (vec![], vec![scope(1); 2]),
        ((1..=33).map(resource).collect(), vec![]),
        (vec![], (1..=129).map(scope).collect()),
    ] {
        assert!(
            ClientSpec::new(
                Label::new("Client").unwrap(),
                true,
                redirects(),
                resources,
                scopes,
                "client_secret_basic"
            )
            .is_err()
        );
    }
}
#[test]
fn callback_allowlist_is_exact_and_never_normalizes_a_request() {
    let r = redirects();
    assert_eq!(r.values().len(), 1);
    assert!(r.allows("https://client.example/cb?x=1"));
    for value in [
        "https://client.example/cb?x=2",
        "https://CLIENT.example/cb?x=1",
        "https://client.example/cb?x=%31",
        "https://client.example/cb?x=1#fragment",
    ] {
        assert!(!r.allows(value));
    }
    for values in [
        vec![],
        vec!["".into()],
        vec!["x".repeat(2049)],
        vec!["https://client.example/cb".into(); 2],
        (0..9)
            .map(|i| format!("https://client.example/{i}"))
            .collect(),
    ] {
        assert!(Redirects::from_validated_urls(values).is_err());
    }
}
fn session() -> SessionFacts {
    SessionFacts {
        active: true,
        credential_live: true,
        revoked: false,
        issued_epoch: 0,
        current_epoch: 0,
        created_ms: 1000,
        seen_ms: 1000,
        expires_ms: 1_000_000,
    }
}
#[test]
fn administrator_and_recent_authentication_are_separate_live_requirements() {
    assert_eq!(administrator(session(), true, 1000, true), Ok(()));
    assert_eq!(
        administrator(session(), false, 1000, true),
        Err(RegistrationError::Forbidden)
    );
    assert_eq!(
        administrator(
            SessionFacts {
                current_epoch: 1,
                ..session()
            },
            true,
            1000,
            true
        ),
        Err(RegistrationError::Unauthorized)
    );
    assert_eq!(
        administrator(session(), true, 999, true),
        Err(RegistrationError::Unauthorized)
    );
    assert_eq!(administrator(session(), true, 300_999, true), Ok(()));
    assert_eq!(
        administrator(session(), true, 301_000, true),
        Err(RegistrationError::RecentAuthenticationRequired)
    );
    assert_eq!(administrator(session(), true, 301_000, false), Ok(()));
}
#[test]
fn revisions_and_secret_overlap_have_exact_bounds() {
    assert_eq!(next_revision(0, 0), Ok(1));
    assert_eq!(next_revision(2, 1), Err(RegistrationError::Conflict));
    assert!(next_revision(i64::MAX as u64, i64::MAX as u64).is_err());
    assert!(next_revision(u64::MAX, u64::MAX).is_err());
    assert_eq!(overlap_deadline(1000, 0), Ok(1000));
    assert_eq!(overlap_deadline(1000, 300), Ok(301_000));
    assert!(overlap_deadline(1000, 301).is_err());
    assert!(overlap_deadline(u64::MAX, 1).is_err());
    assert!(overlap_deadline(i64::MAX as u64, 1).is_err());
    assert!(secret_live(1000, None, false, 1000));
    assert!(secret_live(1000, Some(2000), false, 1999));
    assert!(!secret_live(1000, Some(2000), false, 2000));
    assert!(!secret_live(1000, None, false, 999));
    assert!(!secret_live(1000, None, true, 1000));
}
#[test]
fn scope_grants_require_the_exact_application_and_explicit_resource() {
    let s = spec();
    assert_eq!(
        grants_match(
            app(1),
            &s,
            &[(resource(1), app(1))],
            &[(scope(1), resource(1), app(1))]
        ),
        Ok(())
    );
    for (r, s) in [
        (vec![], vec![]),
        (vec![(resource(1), app(2))], vec![]),
        (
            vec![(resource(1), app(1))],
            vec![(scope(1), resource(1), app(2))],
        ),
        (
            vec![(resource(1), app(1))],
            vec![(scope(1), resource(2), app(1))],
        ),
    ] {
        assert!(grants_match(app(1), &spec(), &r, &s).is_err());
    }
}
