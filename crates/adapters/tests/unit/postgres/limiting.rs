use super::*;
#[test]
fn fence_increments_epoch_and_extends_wait_without_nonce_reuse_or_overflow() {
    let (g, deadline) = next_fence(None, [1; 16], 0).unwrap();
    assert_eq!(g.epoch(), 1);
    assert_eq!(deadline, 904000);
    let state = Enforcement {
        generation: g,
        active: false,
        not_before_ms: deadline,
        now_ms: 0,
        identity: None,
    };
    assert_eq!(next_fence(Some(state), [2; 16], 1).unwrap().0.epoch(), 2);
    assert!(next_fence(Some(state), [1; 16], 1).is_err());
    assert!(next_fence(None, [0; 16], 1).is_err());
    assert!(next_fence(None, [1; 16], -1).is_err());
    assert!(next_fence(None, [1; 16], i64::MAX).is_err());
    assert!(
        next_fence(
            Some(Enforcement {
                not_before_ms: deadline + 1,
                ..state
            }),
            [2; 16],
            0
        )
        .is_err()
    );
    assert!(
        next_fence(
            Some(Enforcement {
                generation: Generation::new(9_007_199_254_740_991, [1; 16]).unwrap(),
                ..state
            }),
            [2; 16],
            1
        )
        .is_err()
    );
}

#[test]
fn activation_requires_the_exact_waited_generation() {
    let g = Generation::new(1, [1; 16]).unwrap();
    let ready = Enforcement {
        generation: g,
        active: false,
        not_before_ms: 1000,
        now_ms: 1000,
        identity: None,
    };
    assert_eq!(activation_matches(ready, g), Ok(()));
    assert!(activation_matches(ready, Generation::new(2, [2; 16]).unwrap()).is_err());
    assert!(
        activation_matches(
            Enforcement {
                now_ms: 999,
                ..ready
            },
            g
        )
        .is_err()
    );
    assert!(
        activation_matches(
            Enforcement {
                active: true,
                ..ready
            },
            g
        )
        .is_err()
    );
}
