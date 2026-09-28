use super::*;
use crate::postgres::operator_client_creation::CreatedClient;
use darkhorse_application::registration::ClientRecord;
use darkhorse_domain::{
    identity::{ApplicationId, ClientId, ClientSecretId},
    registration::{ClientSpec, Label, Redirects},
};
use zeroize::Zeroizing;

#[test]
fn successful_creation_keeps_secret_out_of_metadata_and_returns_one_time_payload() {
    let secret = "fixture-only-secret-value";
    let created = CreatedClient {
        record: ClientRecord {
            id: ClientId::from_u128(2).unwrap(),
            application: ApplicationId::from_u128(3).unwrap(),
            revision: 0,
            spec: ClientSpec::new(
                Label::new("Private client name").unwrap(),
                true,
                Redirects::from_validated_urls(vec!["https://private.example/cb".into()]).unwrap(),
                vec![],
                vec![],
                "client_secret_basic",
            )
            .unwrap(),
            secrets: vec![],
        },
        secret_id: ClientSecretId::from_u128(4).unwrap(),
        secret: Zeroizing::new(secret.to_owned()),
    };
    let output = output(OperationId::from_u128(1).unwrap(), created);
    assert_eq!(output.data.as_object().unwrap().len(), 6);
    assert_eq!(output.data["revision"], "0");
    assert!(!output.data.to_string().contains(secret));
    assert!(!output.data.to_string().contains("private.example"));
    let json = crate::operator::output::sensitive_json(&output).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(value["data"]["client_secret"], secret);
    assert_eq!(
        value["data"]["secret_id"],
        "00000000-0000-0000-0000-000000000004"
    );
    for error in [
        Error::Invalid,
        Error::Denied,
        Error::NotFound,
        Error::Conflict,
        Error::Uncertain,
        Error::Unavailable,
    ] {
        let failure = failure(error, OperationId::from_u128(1).unwrap());
        let encoded = crate::operator::output::render_failure(
            &failure,
            crate::operator::output::Format::Json,
        )
        .unwrap();
        let encoded: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        assert!(encoded.to_string().find(secret).is_none());
        if error == Error::Uncertain {
            assert!(
                encoded["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("lost secret")
            );
        }
    }
}
