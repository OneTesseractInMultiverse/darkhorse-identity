use super::*;

#[test]
fn delivery_results_distinguish_acceptance_retries_and_terminal_failures() {
    assert_eq!(
        outcome(DeliveryResult::Accepted, 1, 10, 20),
        Outcome::Delivered
    );
    assert_eq!(
        outcome(DeliveryResult::Rejected, 1, 10, 20),
        Outcome::Failed
    );
    assert_eq!(
        outcome(DeliveryResult::Retry, 1, 10, 20),
        Outcome::RetryAt(5_020)
    );
    for attempt in 1..MAX_ATTEMPTS {
        assert!(matches!(
            outcome(DeliveryResult::Retry, attempt, 0, 100),
            Outcome::RetryAt(_)
        ));
    }
    assert_eq!(
        outcome(DeliveryResult::Retry, MAX_ATTEMPTS, 0, 100),
        Outcome::Failed
    );
}

#[test]
fn invalid_clocks_expiry_and_counter_overflow_fail_closed() {
    for result in [
        DeliveryResult::Accepted,
        DeliveryResult::Retry,
        DeliveryResult::Rejected,
    ] {
        assert_eq!(outcome(result, 0, 0, 0), Outcome::Failed);
        assert_eq!(outcome(result, 1, 100, 99), Outcome::Failed);
        assert_eq!(outcome(result, 1, 0, MAX_AGE_MS), Outcome::Failed);
        assert_eq!(outcome(result, 1, u64::MAX - 1, u64::MAX), Outcome::Failed);
    }
    assert_eq!(
        outcome(DeliveryResult::Retry, 1, 0, MAX_AGE_MS - 1),
        Outcome::Failed
    );
}

#[test]
fn claiming_bounds_attempts_age_and_lease_arithmetic() {
    assert_eq!(claim(0, 100, 100), Some((1, 30100)));
    assert_eq!(claim(7, 100, 101), Some((8, 30101)));
    assert_eq!(claim(8, 100, 101), None);
    assert_eq!(claim(0, 100, 99), None);
    assert_eq!(claim(0, 0, MAX_AGE_MS), None);
    assert_eq!(claim(0, u64::MAX - 1, u64::MAX), None);
}
