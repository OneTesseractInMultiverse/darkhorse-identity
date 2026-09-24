use super::*;
use darkhorse_application::signing_operations::Completion;
#[test]
fn projections_distinguish_historical_completion_from_current_state() {
    for (kind, name, state) in [
        (Kind::Generate, "signing.generate", "staged"),
        (Kind::Import, "signing.import", "staged"),
        (Kind::Activate, "signing.activate", "active"),
        (Kind::Retire, "signing.retire", "retired"),
    ] {
        let intent = Intent {
            id: OperationId::from_u128(1).unwrap(),
            issuer: "https://issuer.example".into(),
            kid: "public".into(),
            kind,
            expected_revision: 2,
        };
        let success = completed(&intent, 3);
        assert_eq!(success["phase"], state);
        assert_eq!(success["operation"], name);
        for done in [false, true] {
            let value = project(Attempt {
                intent: intent.clone(),
                database_role: "operator".into(),
                prepared_ms: 1,
                completion: done.then_some(Completion {
                    revision: 3,
                    completed_ms: 2,
                }),
                current_revision: Some(9),
                current_phase: Some(Phase::Retiring),
                database_ms: 3,
            });
            assert_eq!(
                value["recorded_outcome"],
                if done { "completed" } else { "pending" }
            );
            assert_eq!(
                value["completed_revision"],
                if done {
                    serde_json::json!(3)
                } else {
                    serde_json::Value::Null
                }
            );
            assert_eq!(value["current_revision"], 9);
            assert_eq!(value["current_phase"], "retiring");
            assert!(value.get("ciphertext").is_none());
            assert!(value.get("wrap_digest").is_none());
        }
        for error in [
            Error::Uncertain,
            Error::Rejected(darkhorse_domain::signing::KeyError::NotFound),
        ] {
            assert_eq!(
                failure(error, intent.id).data.unwrap()["operation_id"],
                success["operation_id"]
            );
        }
    }
}
