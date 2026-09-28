use super::*;
use crate::operator::output::{Format, render};
use darkhorse_application::admin_catalog::Target;
use darkhorse_domain::identity::{OperationId, RoleId};

#[test]
fn success_output_is_bounded_to_commit_evidence_and_target_identity() {
    let operation = OperationId::from_u128(1).unwrap();
    let unchanged = output(
        operation,
        8,
        Written {
            target: Target::Role(RoleId::from_u128(4).unwrap()),
            policy_revision: 8,
        },
    );
    assert_eq!(unchanged.data["completed"], true);
    assert_eq!(unchanged.data["changed"], false);
    assert_eq!(unchanged.data["policy_revision"], "8");
    assert_eq!(unchanged.data.as_object().unwrap().len(), 5);

    let changed = output(
        operation,
        8,
        Written {
            target: Target::Role(RoleId::from_u128(4).unwrap()),
            policy_revision: 9,
        },
    );
    assert_eq!(changed.data["changed"], true);
    let rendered = String::from_utf8(render(&changed, Format::Json).unwrap()).unwrap();
    assert!(rendered.contains("00000000-0000-0000-0000-000000000004"));
    assert!(!rendered.contains("password"));
}

#[test]
fn failures_expose_only_a_fixed_message_and_operation_correlation() {
    let operation = OperationId::from_u128(1).unwrap();
    for error in [
        Error::Denied,
        Error::Invalid,
        Error::Conflict,
        Error::NotFound,
    ] {
        let failure = failure(error, operation);
        let rendered = crate::operator::output::render_failure(&failure, Format::Json).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&rendered).unwrap();
        assert_eq!(
            value["data"]["operation_id"],
            "00000000-0000-0000-0000-000000000001"
        );
        assert!(value["data"].get("target").is_none());
        assert!(value["data"].get("reason").is_none());
    }
}
