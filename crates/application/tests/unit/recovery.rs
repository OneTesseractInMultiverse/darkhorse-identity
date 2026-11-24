use super::{Action, Decision, decide};

#[test]
fn a_serving_permit_allows_serving_or_a_new_fence() {
    assert_eq!(decide(true, None, Action::Serve), Ok(Decision::Serve));
    assert_eq!(
        decide(true, None, Action::Begin("receipt-a")),
        Ok(Decision::Fence)
    );
}

#[test]
fn quarantine_only_acknowledges_the_same_authenticated_operation() {
    assert!(decide(false, Some("receipt-a"), Action::Serve).is_err());
    assert!(decide(false, Some("receipt-a"), Action::Begin("receipt-b")).is_err());
    assert_eq!(
        decide(false, Some("receipt-a"), Action::Begin("receipt-a")),
        Ok(Decision::Confirm)
    );
}

#[test]
fn missing_or_ambiguous_authority_denies_every_action() {
    for (permit, receipt) in [(false, None), (true, Some("receipt-a"))] {
        assert!(decide(permit, receipt, Action::Serve).is_err());
        assert!(decide(permit, receipt, Action::Begin("receipt-a")).is_err());
    }
}
