use super::*;
fn target() -> PrincipalId {
    PrincipalId::from_u128(1).unwrap()
}
#[test]
fn all_operations_require_current_administrator_and_bounded_password_proof() {
    let valid = Authority {
        credential_current: true,
        administrator: true,
        observed_ms: 1000,
    };
    assert_eq!(authorize(valid, 1000), Ok(()));
    assert_eq!(authorize(valid, 60_999), Ok(()));
    for (facts, now) in [
        (valid, 999),
        (valid, 61_000),
        (valid, u64::MAX),
        (
            Authority {
                credential_current: false,
                ..valid
            },
            1000,
        ),
        (
            Authority {
                administrator: false,
                ..valid
            },
            1000,
        ),
    ] {
        assert_eq!(authorize(facts, now), Err(Error::Denied));
    }
    assert_eq!(
        authorize(
            Authority {
                observed_ms: u64::MAX,
                ..valid
            },
            u64::MAX
        ),
        Ok(())
    );
}
#[test]
fn mutation_reason_and_revision_are_bounded_before_any_effects() {
    let show = Operation::Show(target());
    assert_eq!(Request::new(show, None).unwrap().reason(), None);
    let change = Operation::Change {
        target: target(),
        revision: 0,
        action: AccountAction::RevokeAll,
    };
    assert!(matches!(Request::new(change, None), Err(Error::Invalid)));
    for reason in [
        "",
        "   ",
        "line\nforged",
        "\u{202e}hidden",
        "\u{2066}hidden",
    ] {
        assert!(matches!(
            Request::new(change, Some(reason)),
            Err(Error::Invalid)
        ));
    }
    let r = Request::new(change, Some("  Incident INC-123  ")).unwrap();
    assert_eq!(r.reason(), Some("Incident INC-123"));
    assert!(Request::new(change, Some(&"x".repeat(200))).is_ok());
    assert!(Request::new(change, Some(&"x".repeat(201))).is_err());
    assert!(Request::new(change, Some(&"界".repeat(171))).is_err());
    assert!(
        Request::new(
            Operation::Change {
                target: target(),
                revision: i64::MAX as u64 + 1,
                action: AccountAction::RevokeAll
            },
            Some("incident")
        )
        .is_err()
    );
    assert!(
        Request::new(
            Operation::Change {
                target: target(),
                revision: i64::MAX as u64,
                action: AccountAction::RevokeAll
            },
            Some("incident")
        )
        .is_ok()
    );
}
