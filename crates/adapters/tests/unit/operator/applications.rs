use super::*;
use crate::operator::output::{Format, render, render_failure};
use darkhorse_domain::identity::{ApplicationId, PrincipalId};
#[test]
fn mutation_output_contains_only_committed_identity_revision_and_correlation() {
    let result = output(
        OperationId::from_u128(1).unwrap(),
        ApplicationRecord {
            id: ApplicationId::from_u128(2).unwrap(),
            name: "private-name".into(),
            owner: PrincipalId::from_u128(3).unwrap(),
            owner_email: "private@example.com".into(),
            active: false,
            revision: i64::MAX as u64,
        },
    );
    assert_eq!(result.data.as_object().unwrap().len(), 4);
    assert_eq!(result.data["revision"], i64::MAX.to_string());
    assert_eq!(result.data["completed"], true);
    for format in [Format::Human, Format::Json] {
        let value = String::from_utf8(render(&result, format).unwrap()).unwrap();
        assert!(!value.contains("private"));
        assert!(value.contains("00000000-0000-0000-0000-000000000002"));
    }
}
#[test]
fn failures_have_fixed_diagnostics_and_preserve_uncertainty_without_success_fields() {
    let id = OperationId::from_u128(1).unwrap();
    for error in [
        Error::Invalid,
        Error::Denied,
        Error::NotFound,
        Error::Conflict,
        Error::PolicyRejected,
        Error::Unavailable,
        Error::Uncertain,
        Error::Limited { retry_after_ms: 10 },
    ] {
        let failure = failure(error, id);
        assert_eq!(failure.exit_code(), 1);
        let bytes = render_failure(&failure, Format::Json).unwrap();
        let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(result["ok"], false);
        assert_eq!(
            result["data"]["operation_id"],
            uuid::Uuid::from_u128(1).to_string()
        );
        assert!(result["data"].get("application_id").is_none());
        if error == Error::Uncertain {
            assert!(
                result["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("unknown")
            );
        }
        assert_eq!(
            result["data"]["retry_after_ms"].as_u64(),
            if matches!(error, Error::Limited { .. }) {
                Some(10)
            } else {
                None
            }
        );
    }
}
