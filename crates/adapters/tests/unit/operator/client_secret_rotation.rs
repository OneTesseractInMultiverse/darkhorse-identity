use super::*;
use darkhorse_domain::{
    identity::{ApplicationId, ClientId, ClientSecretId, OperationId},
    operator_accounts::Error,
    operator_client_secrets::Target,
};
use zeroize::Zeroizing;

fn request() -> Request {
    Request::new(
        Target {
            application: ApplicationId::from_u128(1).unwrap(),
            client: ClientId::from_u128(2).unwrap(),
        },
        7,
        30,
        "Planned credential rotation",
    )
    .unwrap()
}

fn rotated() -> crate::postgres::operator_client_rotation::RotatedSecret {
    crate::postgres::operator_client_rotation::RotatedSecret {
        revision: 8,
        secret_id: ClientSecretId::from_u128(3).unwrap(),
        secret: Zeroizing::new("a".repeat(64)),
    }
}

#[test]
fn successful_rotation_is_marked_for_one_time_json_delivery_only() {
    let output = output(OperationId::from_u128(4).unwrap(), &request(), rotated());
    assert_eq!(output.data["completed"], true);
    assert_eq!(output.data["revision"], "8");
    assert_eq!(
        output.data["secret_id"],
        "00000000-0000-0000-0000-000000000003"
    );
    assert_eq!(output.data["overlap_seconds"], 30);
    assert!(output.data.get("client_secret").is_none());

    let bytes = super::super::output::sensitive_json(&output).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["data"]["client_secret"], "a".repeat(64));
    assert!(super::super::output::render(&output, super::super::output::Format::Json).is_err());
}

#[test]
fn failure_messages_and_records_never_include_secret_material() {
    let secret = "sensitive-secret-marker";
    for error in [
        Error::Invalid,
        Error::Denied,
        Error::NotFound,
        Error::Conflict,
        Error::Uncertain,
        Error::Unavailable,
    ] {
        let output = failure(error, OperationId::from_u128(4).unwrap());
        let rendered =
            super::super::output::render_failure(&output, super::super::output::Format::Json)
                .unwrap();
        let rendered = String::from_utf8(rendered).unwrap();
        assert!(!rendered.contains(secret));
        assert!(!rendered.contains("client_secret"));
    }
}
