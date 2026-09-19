use super::*;

fn family() -> Family {
    Family {
        created: 1_000,
        expires: 1_000 + FAMILY_MS,
        revoked: false,
    }
}
fn member() -> Member {
    Member {
        created: 1_000,
        expires: 1_000 + IDLE_MS,
        generation: 0,
        consumed: false,
    }
}
#[test]
fn rotation_is_single_use_bounded_and_does_not_extend_the_family() {
    let family = family();
    assert_eq!(
        rotate(&family, &member(), 2_000),
        Ok(Step::Next(Window {
            generation: 1,
            refresh_expires: 2_000 + IDLE_MS,
            access_expires: 2_000 + tokens::ACCESS_MS,
        }))
    );
    let mut consumed = member();
    consumed.consumed = true;
    assert_eq!(
        rotate(&family, &consumed, consumed.expires),
        Ok(Step::Replay)
    );
    assert_eq!(
        rotate(&family, &member(), member().expires),
        Err(Error::InvalidGrant)
    );
    let mut last = member();
    last.generation = MAX_GENERATION;
    assert_eq!(rotate(&family, &last, 2_000), Err(Error::InvalidGrant));
    let mut ending = member();
    ending.created = family.expires - 10;
    ending.expires = family.expires;
    assert_eq!(
        rotate(&family, &ending, family.expires - 1),
        Ok(Step::Next(Window {
            generation: 1,
            refresh_expires: family.expires,
            access_expires: family.expires,
        }))
    );
}
#[test]
fn invalid_or_dead_families_and_incoherent_members_deny_even_replays() {
    for now in [999, family().expires] {
        assert_eq!(rotate(&family(), &member(), now), Err(Error::InvalidGrant));
    }
    let mut f = family();
    f.revoked = true;
    assert_eq!(rotate(&f, &member(), 2_000), Err(Error::InvalidGrant));
    for (created, expires) in [(1_000, 1_000), (1_000, 999), (1_000, 1_001 + FAMILY_MS)] {
        let f = Family {
            created,
            expires,
            revoked: false,
        };
        assert_eq!(rotate(&f, &member(), 2_000), Err(Error::InvalidGrant));
    }
    for m in [
        Member {
            created: 999,
            ..member()
        },
        Member {
            created: 3_000,
            ..member()
        },
        Member {
            expires: 1_000,
            ..member()
        },
        Member {
            expires: 1_001 + IDLE_MS,
            ..member()
        },
        Member {
            generation: MAX_GENERATION + 1,
            ..member()
        },
    ] {
        assert_eq!(rotate(&family(), &m, 2_000), Err(Error::InvalidGrant));
    }
    let f = Family {
        expires: 2_500,
        ..family()
    };
    assert_eq!(rotate(&f, &member(), 2_000), Err(Error::InvalidGrant));
}
#[test]
fn initial_expiry_is_bound_to_authentication_and_retention_is_checked() {
    assert_eq!(
        initial(1_000, 2_000),
        Ok(Family {
            created: 2_000,
            expires: 1_000 + FAMILY_MS,
            revoked: false
        })
    );
    assert_eq!(initial(2_000, 1_000), Err(Error::InvalidGrant));
    assert_eq!(initial(0, FAMILY_MS), Err(Error::InvalidGrant));
    assert_eq!(
        initial(i64::MAX as u64, i64::MAX as u64),
        Err(Error::Unavailable)
    );
    assert_eq!(retention_cutoff(RETENTION_MS - 1), None);
    assert_eq!(retention_cutoff(RETENTION_MS), Some(0));
}
#[test]
fn refresh_scopes_can_only_narrow_the_previous_generation() {
    let original = ["openid", "profile", "email"].map(String::from);
    assert_eq!(scopes(&original, None, None), Ok(original.to_vec()));
    let narrowed = ["openid", "email"].map(String::from);
    assert_eq!(
        scopes(&original, Some(&narrowed), None),
        Ok(narrowed.to_vec())
    );
    assert_eq!(
        scopes(&narrowed, Some(&original), None),
        Err(Error::InvalidScope)
    );
    for invalid in [
        vec![],
        vec!["email".into()],
        vec!["openid".into(), "openid".into()],
    ] {
        assert_eq!(
            scopes(&original, Some(&invalid), None),
            Err(Error::InvalidScope)
        );
    }
    let resource = ["openid", "read", "write"].map(String::from);
    let read = ["openid", "read"].map(String::from);
    assert_eq!(
        scopes(&resource, Some(&read), Some("resource")),
        Ok(read.to_vec())
    );
    assert_eq!(
        scopes(&read, Some(&resource), Some("resource")),
        Err(Error::InvalidScope)
    );
    assert_eq!(scopes(&[], None, None), Err(Error::InvalidScope));
}
