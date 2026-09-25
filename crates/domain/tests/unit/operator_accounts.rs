use super::*;
fn target() -> PrincipalId {
    PrincipalId::from_u128(1).unwrap()
}
#[test]
fn protected_outcomes_require_current_authority_including_state_dependent_errors() {
    assert!(needs_current_authority(&Ok::<_, Error>(())));
    for error in [
        Error::Invalid,
        Error::NotFound,
        Error::Conflict,
        Error::PolicyRejected,
    ] {
        assert!(needs_current_authority(&Err::<(), _>(error)));
    }
    for error in [
        Error::Denied,
        Error::Limited {
            retry_after_ms: 1000,
        },
        Error::Unavailable,
        Error::Uncertain,
    ] {
        assert!(!needs_current_authority(&Err::<(), _>(error)));
    }
}
#[test]
fn completion_accepts_only_the_requested_self_transition() {
    let actor = target();
    let other = PrincipalId::from_u128(2).unwrap();
    let original = ActorState {
        status: AccountStatus::Active,
        epoch: 7,
    };
    for target in [actor, other] {
        for action in [
            AccountAction::RevokeAll,
            AccountAction::SetStatus(AccountStatus::Inactive),
            AccountAction::SetStatus(AccountStatus::Active),
        ] {
            let operation = Operation::Change {
                target,
                revision: 100,
                action,
            };
            assert_eq!(completion_state(actor, 7, operation, false), Ok(original));
            let expected = if target != actor {
                Ok(original)
            } else {
                match action {
                    AccountAction::RevokeAll => Ok(ActorState {
                        epoch: 8,
                        ..original
                    }),
                    AccountAction::SetStatus(AccountStatus::Inactive) => Ok(ActorState {
                        status: AccountStatus::Inactive,
                        epoch: 8,
                    }),
                    AccountAction::SetStatus(AccountStatus::Active) => Err(Error::Denied),
                }
            };
            assert_eq!(completion_state(actor, 7, operation, true), expected);
        }
    }
    assert_eq!(
        completion_state(actor, 7, Operation::Show(actor), false),
        Ok(original)
    );
    let revoke = Operation::Change {
        target: actor,
        revision: 0,
        action: AccountAction::RevokeAll,
    };
    assert_eq!(
        completion_state(actor, i64::MAX as u64 - 1, revoke, true)
            .unwrap()
            .epoch,
        i64::MAX as u64
    );
    for epoch in [i64::MAX as u64, u64::MAX] {
        assert_eq!(
            completion_state(actor, epoch, revoke, true),
            Err(Error::Denied)
        );
    }
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
