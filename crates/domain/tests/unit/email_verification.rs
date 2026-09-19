use super::*;
#[test]
fn requests_have_persistent_cooldown_daily_and_capacity_limits() {
    assert_eq!(request(None, 0, 0, 100), Ok(100 + LIFETIME_MS));
    assert_eq!(
        request(Some(100), 1, 0, 100 + LIFETIME_MS - 1),
        Err(Error::Throttled)
    );
    assert!(request(Some(100), 1, 0, 100 + LIFETIME_MS).is_ok());
    assert_eq!(request(Some(101), 0, 0, 100), Err(Error::Throttled));
    assert_eq!(request(None, 5, 0, 100), Err(Error::Throttled));
    assert_eq!(request(None, 0, MAX_QUEUED, 100), Err(Error::Unavailable));
    assert_eq!(
        request(None, 0, 0, i64::MAX as u64),
        Err(Error::Unavailable)
    );
}
#[test]
fn proof_is_bound_to_current_email_epoch_time_and_single_use() {
    let proof = Proof {
        email: "a@example.com",
        epoch: 1,
        created_ms: 100,
        expires_ms: 100 + LIFETIME_MS,
        consumed: false,
    };
    assert_eq!(redeem(&proof, "a@example.com", 1, 100), Ok(()));
    assert_eq!(redeem(&proof, "b@example.com", 1, 101), Err(Error::Invalid));
    assert_eq!(redeem(&proof, "a@example.com", 2, 101), Err(Error::Invalid));
    assert_eq!(redeem(&proof, "a@example.com", 1, 99), Err(Error::Invalid));
    assert_eq!(
        redeem(&proof, "a@example.com", 1, proof.expires_ms),
        Err(Error::Invalid)
    );
    assert_eq!(
        redeem(
            &Proof {
                consumed: true,
                ..proof
            },
            "a@example.com",
            1,
            101
        ),
        Err(Error::Invalid)
    );
}
#[test]
fn delivery_has_a_finite_retry_budget_and_never_extends_expiry() {
    assert_eq!(lease(100, 100 + LIFETIME_MS), 60100);
    assert_eq!(lease(100, 200), 200);
    assert_eq!(lease(u64::MAX, u64::MAX), u64::MAX);
    for attempt in 1..5 {
        assert!(retry(attempt, 100, 100 + LIFETIME_MS).is_some());
    }
    assert_eq!(retry(0, 100, 100 + LIFETIME_MS), None);
    assert_eq!(retry(5, 100, 100 + LIFETIME_MS), None);
    assert_eq!(retry(1, 100, 100 + 60_000), None);
    assert_eq!(retry(1, u64::MAX, u64::MAX), None);
}
