use super::*;
use darkhorse_application::limiter_activation::Completion;
use darkhorse_domain::limiter_recovery::{Enforcement, Generation, ServerIdentity};
fn attempt() -> Attempt {
    Attempt {
        id: OperationId::from_u128(1).unwrap(),
        generation: Generation::new(1, [1; 16]).unwrap(),
        not_before_ms: 1,
        prepared_ms: 2,
        database_role: "operator".into(),
        completion: None,
        current: Enforcement {
            generation: Generation::new(2, [2; 16]).unwrap(),
            not_before_ms: 4,
            now_ms: 5,
            active: false,
            identity: None,
        },
    }
}
#[test]
fn historical_receipts_and_pending_attempts_never_claim_current_readiness() {
    let pending = project(attempt());
    assert_eq!(pending["recorded_outcome"], "pending");
    assert_eq!(pending["same_generation"], false);
    assert!(pending["completed_ms"].is_null());
    let completed = project(Attempt {
        completion: Some(Completion {
            completed_ms: 3,
            identity: ServerIdentity {
                run: [1; 20],
                replication: [2; 20],
            },
        }),
        ..attempt()
    });
    assert_eq!(completed["recorded_outcome"], "activated");
    assert_eq!(completed["current_phase"], "cooling");
    assert_eq!(completed["current_epoch"], 2);
    assert!(completed.get("run_id").is_none());
}
#[test]
fn failures_are_correlated_and_redacted_without_retry_advice() {
    for error in [
        Error::NotReady,
        Error::NotFound,
        Error::Unavailable,
        Error::Uncertain,
    ] {
        let output = crate::operator::output::render_failure(
            &failure(error, attempt().id),
            crate::operator::output::Format::Json,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(
            value["data"]["operation_id"],
            "00000000-0000-0000-0000-000000000001"
        );
        assert_eq!(value["ok"], false);
    }
}
