use super::*;

fn snapshot() -> AccountSnapshot {
    AccountSnapshot {
        status: AccountStatus::Active,
        credential_epoch: 5,
        revision: 7,
        eligible_administrator: false,
        eligible_administrators: 1,
    }
}

#[test]
fn profile_preserves_display_and_uses_explicit_ascii_login_comparison() {
    let profile = Profile::new(" Alice.Example+team@Example.COM ", " José ", " O’Connor ").unwrap();
    assert_eq!(profile.email(), "Alice.Example+team@Example.COM");
    assert_eq!(profile.email_key(), "alice.example+team@example.com");
    assert_eq!(profile.first_name(), "José");
    assert_eq!(profile.last_name(), "O’Connor");
    for email in [
        "",
        "a",
        "@b.com",
        "a@@b.com",
        ".a@b.com",
        "a..b@c.com",
        "a.@b.com",
        "a b@c.com",
        "ü@b.com",
        "a@-b.com",
        "a@b-.com",
        "a@b..com",
        "a@b_c.com",
        "a@b",
        "a\n@b.com",
    ] {
        assert_eq!(
            Profile::new(email, "A", "B"),
            Err(DirectoryError::Email),
            "{email}"
        );
    }
    for email in [
        format!("{}@example.com", "a".repeat(65)),
        format!("a@{}.com", "b".repeat(64)),
        format!(
            "a@{}.{}.{}.{}.com",
            "b".repeat(63),
            "b".repeat(63),
            "b".repeat(63),
            "b".repeat(63)
        ),
    ] {
        assert_eq!(Profile::new(&email, "A", "B"), Err(DirectoryError::Email));
    }
    // The inclusive boundaries must accept real addresses at both limits.
    let longest = format!(
        "{}@{}.{}.{}",
        "a".repeat(64),
        "b".repeat(63),
        "c".repeat(63),
        "d".repeat(61)
    );
    assert_eq!(longest.len(), 254);
    for email in [format!("{}@example.com", "a".repeat(64)), longest] {
        assert_eq!(Profile::new(&email, "A", "B").unwrap().email(), email);
    }
    for name in [
        "".to_owned(),
        " ".to_owned(),
        "a\nb".to_owned(),
        "a".repeat(101),
    ] {
        for (first, last) in [(&*name, "B"), ("A", &*name)] {
            assert_eq!(
                Profile::new("a@b.com", first, last),
                Err(DirectoryError::Name)
            );
        }
    }
    assert!(Profile::new("a@b.com", &"a".repeat(100), "B").is_ok());
}

#[test]
fn password_bounds_preserve_unicode_and_spaces_without_normalizing() {
    for value in [
        "a".repeat(15),
        "a".repeat(128),
        "🦀".repeat(15),
        " a long passphrase ".to_owned(),
    ] {
        assert_eq!(validate_password(&value), Ok(()));
    }
    for value in [
        "".to_owned(),
        "a".repeat(14),
        "a".repeat(129),
        "long\npassword text".to_owned(),
    ] {
        assert_eq!(validate_password(&value), Err(DirectoryError::Password));
    }
}

#[test]
fn deactivation_advances_epoch_and_reactivation_cannot_restore_old_credentials() {
    let change = plan_change(
        snapshot(),
        AccountAction::SetStatus(AccountStatus::Inactive),
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        change,
        AccountChange {
            status: AccountStatus::Inactive,
            credential_epoch: 6,
            revision: 8
        }
    );
    let inactive = AccountSnapshot {
        status: change.status,
        credential_epoch: change.credential_epoch,
        revision: change.revision,
        ..snapshot()
    };
    assert_eq!(
        plan_change(inactive, AccountAction::SetStatus(AccountStatus::Active)),
        Ok(Some(AccountChange {
            status: AccountStatus::Active,
            credential_epoch: 6,
            revision: 9
        }))
    );
    assert_eq!(
        plan_change(snapshot(), AccountAction::RevokeAll),
        Ok(Some(AccountChange {
            status: AccountStatus::Active,
            credential_epoch: 6,
            revision: 8
        }))
    );
    assert_eq!(
        plan_change(inactive, AccountAction::RevokeAll)
            .unwrap()
            .unwrap()
            .credential_epoch,
        7
    );
    for state in [snapshot(), inactive] {
        assert_eq!(
            plan_change(state, AccountAction::SetStatus(state.status)),
            Ok(None)
        );
    }
}

#[test]
fn last_administrator_and_counter_exhaustion_fail_closed() {
    for count in [0, 1] {
        assert_eq!(
            plan_change(
                AccountSnapshot {
                    eligible_administrator: true,
                    eligible_administrators: count,
                    ..snapshot()
                },
                AccountAction::SetStatus(AccountStatus::Inactive)
            ),
            Err(DirectoryError::LastAdministrator)
        );
    }
    assert!(
        plan_change(
            AccountSnapshot {
                eligible_administrator: true,
                eligible_administrators: 2,
                ..snapshot()
            },
            AccountAction::SetStatus(AccountStatus::Inactive)
        )
        .is_ok()
    );
    assert!(
        plan_change(
            AccountSnapshot {
                eligible_administrator: true,
                ..snapshot()
            },
            AccountAction::RevokeAll
        )
        .is_ok()
    );
    for state in [
        AccountSnapshot {
            credential_epoch: i64::MAX as u64,
            ..snapshot()
        },
        AccountSnapshot {
            revision: i64::MAX as u64,
            ..snapshot()
        },
    ] {
        assert_eq!(
            plan_change(state, AccountAction::RevokeAll),
            Err(DirectoryError::CounterExhausted)
        );
    }
    assert_eq!(
        plan_change(
            AccountSnapshot {
                revision: i64::MAX as u64 - 1,
                credential_epoch: i64::MAX as u64 - 1,
                ..snapshot()
            },
            AccountAction::RevokeAll
        )
        .unwrap()
        .unwrap()
        .revision,
        i64::MAX as u64
    );
}
