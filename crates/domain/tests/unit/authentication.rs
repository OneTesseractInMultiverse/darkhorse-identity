use super::*;

#[test]
fn login_normalizes_only_email_and_bounds_work() {
    assert_eq!(
        login_email(" Admin@Example.COM ").unwrap(),
        "admin@example.com"
    );
    for email in ["", "no-at", "x@localhost", "é@example.com"] {
        assert!(login_email(email).is_err());
    }
    assert!(login_password(" short ").is_ok());
    assert!(login_password(&"é".repeat(256)).is_ok());
    for value in [String::new(), "a".repeat(513), "a\nb".into()] {
        assert!(login_password(&value).is_err());
    }
}

#[test]
fn authentication_requires_all_current_facts() {
    assert!(authenticated(true, true, true));
    for (exists, active, matches) in [
        (false, true, true),
        (true, false, true),
        (true, true, false),
    ] {
        assert!(!authenticated(exists, active, matches));
    }
}

#[test]
fn sessions_expire_at_boundaries_and_reject_clock_regression_and_revocation() {
    let facts = SessionFacts {
        active: true,
        credential_live: true,
        revoked: false,
        issued_epoch: 2,
        current_epoch: 2,
        created_ms: 100,
        seen_ms: 200,
        expires_ms: 100 + ABSOLUTE_MS,
    };
    assert!(session_live(facts, 201));
    assert!(session_live(facts, 200 + IDLE_MS - 1));
    assert!(!session_live(facts, 200 + IDLE_MS));
    assert!(!session_live(facts, 199));
    assert!(!session_live(
        SessionFacts {
            expires_ms: facts.created_ms + ABSOLUTE_MS + 1,
            ..facts
        },
        201
    ));
    assert!(!session_live(
        SessionFacts {
            seen_ms: 99,
            ..facts
        },
        201
    ));
    assert!(!session_live(
        SessionFacts {
            seen_ms: facts.expires_ms - 1,
            ..facts
        },
        facts.expires_ms
    ));
    for changed in [
        SessionFacts {
            active: false,
            ..facts
        },
        SessionFacts {
            credential_live: false,
            ..facts
        },
        SessionFacts {
            revoked: true,
            ..facts
        },
        SessionFacts {
            current_epoch: 3,
            ..facts
        },
    ] {
        assert!(!session_live(changed, 201));
    }
}
