use super::*;
use darkhorse_domain::registration::{ClientSpec, Label, Redirects};
#[test]
fn completion_contains_only_committed_identifiers_and_revision() {
    let result = output(
        OperationId::from_u128(1).unwrap(),
        ClientRecord {
            id: ClientId::from_u128(2).unwrap(),
            application: ApplicationId::from_u128(3).unwrap(),
            revision: i64::MAX as u64,
            spec: ClientSpec::new(
                Label::new("Private name").unwrap(),
                true,
                Redirects::from_validated_urls(vec!["https://private.example/cb".into()]).unwrap(),
                vec![],
                vec![],
                "client_secret_basic",
            )
            .unwrap(),
            secrets: vec![],
        },
    );
    assert_eq!(result.data.as_object().unwrap().len(), 5);
    assert_eq!(result.data["revision"], i64::MAX.to_string());
    assert!(!result.data.to_string().contains("Private"));
    assert!(!result.data.to_string().contains("private.example"));
}
#[test]
fn failures_keep_uncertain_outcomes_explicit_without_configuration_or_success_fields() {
    for error in [
        Error::Invalid,
        Error::Denied,
        Error::NotFound,
        Error::Conflict,
        Error::Uncertain,
        Error::Unavailable,
        Error::PolicyRejected,
        Error::Limited { retry_after_ms: 42 },
    ] {
        let value = failure(error, OperationId::from_u128(1).unwrap());
        let bytes =
            super::super::output::render_failure(&value, super::super::output::Format::Json)
                .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["ok"], false);
        assert!(value["data"].get("client_id").is_none());
        if error == Error::Uncertain {
            assert!(
                value["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("unknown")
            );
        }
        if matches!(error, Error::Limited { .. }) {
            assert_eq!(value["data"]["retry_after_ms"], 42);
        }
    }
}
