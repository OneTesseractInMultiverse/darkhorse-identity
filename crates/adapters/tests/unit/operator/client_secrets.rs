use super::*;
use darkhorse_application::operator_client_secrets::{Inventory, Metadata};
use darkhorse_domain::identity::{ApplicationId, ClientId, ClientSecretId};
fn target() -> Target {
    Target {
        application: ApplicationId::from_u128(1).unwrap(),
        client: ClientId::from_u128(2).unwrap(),
    }
}
fn request(operation: Operation) -> Request {
    Request::new(
        target(),
        operation,
        if matches!(operation, Operation::Retire { .. }) {
            Some("Rotate exposed credential")
        } else {
            None
        },
    )
    .unwrap()
}
fn inventory() -> Outcome {
    Outcome::Listed(Inventory {
        revision: i64::MAX as u64,
        observed_ms: 20,
        items: vec![Metadata {
            id: ClientSecretId::from_u128(3).unwrap(),
            created_ms: 10,
            expires_ms: Some(15),
            retired: false,
        }],
        next: None,
    })
}
#[test]
fn inventory_contains_only_lifecycle_metadata_and_preserves_full_revision_precision() {
    let output = output(
        OperationId::from_u128(4).unwrap(),
        &request(Operation::List {
            after: None,
            limit: 1,
        }),
        inventory(),
    )
    .unwrap();
    assert_eq!(output.data["revision"], i64::MAX.to_string());
    assert_eq!(output.data["observed_ms"], 20);
    assert_eq!(output.data["next"], serde_json::Value::Null);
    assert_eq!(
        output.data["items"][0],
        serde_json::json!({"id":id(3),"created_ms":10,"expires_ms":15,"retired":false})
    );
    assert_eq!(output.data.as_object().unwrap().len(), 7);
}
#[test]
fn retirement_returns_committed_revision_without_fabricating_completion_for_wrong_outcome() {
    let retire = request(Operation::Retire {
        secret: ClientSecretId::from_u128(3).unwrap(),
        revision: 0,
    });
    let operation = OperationId::from_u128(4).unwrap();
    let out = output(operation, &retire, Outcome::Retired { revision: 1 }).unwrap();
    assert_eq!(out.data["completed"], true);
    assert_eq!(out.data["revision"], "1");
    assert_eq!(out.data["secret_id"], id(3));
    assert_eq!(out.data.as_object().unwrap().len(), 6);
    assert!(matches!(
        output(operation, &retire, inventory()),
        Err(Error::Unavailable)
    ));
    assert!(matches!(
        output(
            operation,
            &request(Operation::List {
                after: None,
                limit: 1
            }),
            Outcome::Retired { revision: 1 }
        ),
        Err(Error::Unavailable)
    ));
    let mut page = match inventory() {
        Outcome::Listed(page) => page,
        _ => unreachable!(),
    };
    page.items.push(page.items[0].clone());
    assert!(matches!(
        output(
            operation,
            &request(Operation::List {
                after: None,
                limit: 1
            }),
            Outcome::Listed(page)
        ),
        Err(Error::Unavailable)
    ));
}
#[test]
fn failures_keep_unknown_commit_and_attempt_limits_explicit() {
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
        let out = failure(error, OperationId::from_u128(4).unwrap());
        let bytes =
            super::super::output::render_failure(&out, super::super::output::Format::Json).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["ok"], false);
        assert!(value["data"].get("completed").is_none());
        assert!(value["data"].get("secret_id").is_none());
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
