use super::*;

fn state() -> Enforcement {
    Enforcement {
        generation: Generation::new(1, [1; 16]).unwrap(),
        active: true,
        not_before_ms: 1,
        now_ms: 2_000_000,
        identity: Some(ServerIdentity {
            run: [1; 20],
            replication: [2; 20],
        }),
    }
}

#[test]
fn recovery_wait_covers_every_window_and_in_flight_operation() {
    assert_eq!(recovery_deadline(10), Ok(904_010));
    assert!(recovery_deadline(u64::MAX).is_err());
    assert!(recovery_deadline(9_007_199_254_740_991 - 903_999).is_err());
    assert_eq!(
        recovery_deadline(9_007_199_254_740_991 - 904_000),
        Ok(9_007_199_254_740_991)
    );
    let mut s = state();
    assert!(activation_ready(s).is_err());
    s.active = false;
    s.not_before_ms = s.now_ms + 1;
    assert!(activation_ready(s).is_err());
    s.now_ms += 1;
    assert_eq!(activation_ready(s), Ok(()));
    s.now_ms = u64::MAX;
    assert!(activation_ready(s).is_err());
}

#[test]
fn identity_generation_and_post_charge_fences_cannot_be_reused() {
    assert!(Generation::new(0, [1; 16]).is_err());
    assert!(Generation::new(1, [0; 16]).is_err());
    assert!(Generation::new(9_007_199_254_740_992, [1; 16]).is_err());
    let maximum = Generation::new(9_007_199_254_740_991, [255; 16]).unwrap();
    assert_eq!(maximum.epoch(), 9_007_199_254_740_991);
    assert_eq!(maximum.nonce(), [255; 16]);
    let s = state();
    assert_eq!(confirm(s, s), Ok(()));
    for changed in [
        Enforcement { active: false, ..s },
        Enforcement {
            identity: None,
            ..s
        },
        Enforcement {
            identity: Some(ServerIdentity {
                run: [3; 20],
                replication: [2; 20],
            }),
            ..s
        },
        Enforcement {
            generation: Generation::new(2, [1; 16]).unwrap(),
            ..s
        },
        Enforcement {
            generation: Generation::new(1, [2; 16]).unwrap(),
            ..s
        },
        Enforcement {
            now_ms: s.now_ms - 1,
            ..s
        },
        Enforcement {
            now_ms: s.now_ms + 1001,
            ..s
        },
        Enforcement {
            not_before_ms: s.now_ms + 1,
            ..s
        },
    ] {
        assert!(confirm(s, changed).is_err());
    }
    assert!(confirm(Enforcement { active: false, ..s }, s).is_err());
    assert_eq!(
        confirm(
            s,
            Enforcement {
                now_ms: s.now_ms + 1000,
                ..s
            }
        ),
        Ok(())
    );
}

#[test]
fn server_and_database_clocks_must_agree_and_never_rollback() {
    assert_eq!(trusted_time(2000, 3000, 1000), Ok(3000));
    assert_eq!(trusted_time(3000, 2000, 1000), Ok(2000));
    assert_eq!(trusted_time(2000, 2000, 2000), Ok(2000));
    for values in [
        (2000, 3001, 0),
        (3001, 2000, 0),
        (2000, 2000, 2001),
        (u64::MAX, u64::MAX, 0),
    ] {
        assert!(trusted_time(values.0, values.1, values.2).is_err());
    }
}

#[test]
fn exact_time_ceiling_and_activation_boundary_remain_inclusive() {
    let max = 9_007_199_254_740_991;
    let s = Enforcement {
        now_ms: max,
        not_before_ms: max,
        ..state()
    };
    assert_eq!(activation_ready(Enforcement { active: false, ..s }), Ok(()));
    assert_eq!(require_active(s), s.identity.ok_or(LimitError::UnsafeState));
    assert!(
        require_active(Enforcement {
            now_ms: max + 1,
            ..s
        })
        .is_err()
    );
    assert_eq!(trusted_time(max, max, max), Ok(max));
    assert!(trusted_time(max + 1, max, 0).is_err());
    assert!(trusted_time(max, max + 1, 0).is_err());
}
