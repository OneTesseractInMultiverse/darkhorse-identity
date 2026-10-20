use super::*;

#[test]
fn logout_lifetime_is_two_minutes_and_numeric_dates_remain_exact() {
    assert_eq!(logout_expiration(0), Ok(120));
    assert_eq!(logout_expiration(1001), Ok(1121));
    assert_eq!(
        logout_expiration(9_007_199_254_740_871),
        Ok(9_007_199_254_740_991)
    );
    for issued in [9_007_199_254_740_872, u64::MAX - 119, u64::MAX] {
        assert_eq!(logout_expiration(issued), Err(KeyError::Invalid));
    }
}
fn staged() -> KeyState {
    KeyState {
        phase: Phase::Staged,
        created_ms: 1000,
        activated_ms: None,
        verify_until_ms: None,
    }
}
#[test]
fn activation_requires_propagation_time_and_never_reactivates_old_keys() {
    assert_eq!(activate(staged(), 60_999), Err(KeyError::NotReady));
    assert_eq!(activate(staged(), 61_000), Ok(()));
    assert_eq!(activate(staged(), 999), Err(KeyError::Invalid));
    for phase in [Phase::Active, Phase::Retiring, Phase::Retired] {
        assert!(activate(KeyState { phase, ..staged() }, 61_000).is_err());
    }
}
#[test]
fn old_public_keys_survive_verification_window_and_retirement_is_terminal() {
    let active = KeyState {
        phase: Phase::Active,
        activated_ms: Some(61_000),
        ..staged()
    };
    assert_eq!(retire(active, 100_000), Err(KeyError::Conflict));
    assert_eq!(overlap_end(100_000), Ok(700_000));
    assert!(overlap_end(u64::MAX).is_err());
    assert!(overlap_end(i64::MAX as u64).is_err());
    let old = KeyState {
        phase: Phase::Retiring,
        verify_until_ms: Some(700_000),
        ..active
    };
    assert!(published(old, 699_999));
    assert!(!published(old, 700_000));
    assert_eq!(retire(old, 699_999), Err(KeyError::NotReady));
    assert_eq!(retire(old, 700_000), Ok(()));
    assert_eq!(
        retire(
            KeyState {
                phase: Phase::Retired,
                ..old
            },
            700_000
        ),
        Err(KeyError::Conflict)
    );
    assert_eq!(retire(staged(), 1000), Ok(()));
    assert!(!published(staged(), 999));
    assert!(published(staged(), 1000));
    assert!(published(active, 61_000));
    assert!(!published(
        KeyState {
            phase: Phase::Retired,
            ..old
        },
        1_000_000
    ));
    assert!(!published(
        KeyState {
            verify_until_ms: None,
            ..old
        },
        699_999
    ));
}
#[test]
fn signed_message_profiles_have_distinct_types_and_bounded_lifetimes() {
    assert_eq!(Purpose::IdToken.header_type(), "JWT");
    assert_eq!(Purpose::LogoutToken.header_type(), "logout+jwt");
    assert_eq!(message_times(1000, 1001), Ok(()));
    assert_eq!(message_times(1000, 1300), Ok(()));
    for expiry in [999, 1000, 1301, u64::MAX] {
        assert!(message_times(1000, expiry).is_err());
    }
}

#[test]
fn clock_rollback_prevents_retirement() {
    assert_eq!(retire(staged(), 999), Err(KeyError::Invalid));
}
