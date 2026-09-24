use super::*;
use crate::operator::output::{Format, OUTPUT_LIMIT, render};
use darkhorse_application::registration::{ApplicationRecord, ClientRecord, SecretMetadata};
use darkhorse_domain::{
    identity::*,
    registration::{ClientSpec, Label, Redirects},
};
fn application() -> ApplicationRecord {
    ApplicationRecord {
        id: ApplicationId::from_u128(1).unwrap(),
        name: "\u{202e}".repeat(100),
        owner: PrincipalId::from_u128(1).unwrap(),
        owner_email: "x".repeat(254),
        active: false,
        revision: i64::MAX as u64,
    }
}
fn client() -> ClientRecord {
    let redirects = (0..8)
        .map(|i| {
            format!(
                "https://client.example/{i}?exact={}",
                "\u{202e}".repeat(670)
            )
        })
        .collect();
    let mut spec = ClientSpec::new(
        Label::new(&"\u{202e}".repeat(100)).unwrap(),
        false,
        Redirects::from_validated_urls(redirects).unwrap(),
        (1..=32)
            .map(|id| ResourceId::from_u128(id).unwrap())
            .collect(),
        (1..=128)
            .map(|id| ScopeId::from_u128(id).unwrap())
            .collect(),
        "client_secret_basic",
    )
    .unwrap();
    spec.refresh_tokens = true;
    ClientRecord {
        id: ClientId::from_u128(2).unwrap(),
        application: application().id,
        revision: i64::MAX as u64,
        spec,
        secrets: vec![SecretMetadata {
            id: ClientSecretId::from_u128(999).unwrap(),
            created_ms: 1,
            expires_ms: None,
        }],
    }
}
fn target() -> ReadTarget {
    ReadTarget::Client {
        application: application().id,
        client: client().id,
    }
}
#[test]
fn detail_projection_is_exact_bounded_and_escapes_untrusted_terminal_text() {
    for (target, record, fields) in [
        (
            ReadTarget::Application(application().id),
            Record::Application(application()),
            7,
        ),
        (target(), Record::Client(client()), 11),
    ] {
        let output = output(OperationId::from_u128(3).unwrap(), target, record).unwrap();
        assert_eq!(output.data["record"].as_object().unwrap().len(), fields);
        assert_eq!(output.data["record"]["revision"], i64::MAX.to_string());
        assert!(output.data["record"].get("secrets").is_none());
        for format in [Format::Human, Format::Json] {
            let bytes = render(&output, format).unwrap();
            assert!(bytes.len() < OUTPUT_LIMIT);
            let text = String::from_utf8(bytes.clone()).unwrap();
            assert!(!text.contains('\u{202e}'));
            assert!(!text.contains("00000000-0000-0000-0000-0000000003e7"));
            let _: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        }
    }
    let data = project(target(), Record::Client(client())).unwrap();
    assert_eq!(
        data["redirect_uris"],
        serde_json::json!(client().spec.redirects.values())
    );
    assert_eq!(data["resource_ids"].as_array().unwrap().len(), 32);
    assert_eq!(data["scope_ids"].as_array().unwrap().len(), 128);
    assert_eq!(data["refresh_tokens"], true);
    assert_eq!(data["token_endpoint_auth_method"], "client_secret_basic");
}
#[test]
fn detail_projection_rejects_wrong_record_kind_and_foreign_identifiers() {
    assert!(project(target(), Record::Application(application())).is_err());
    assert!(
        project(
            ReadTarget::Application(application().id),
            Record::Client(client())
        )
        .is_err()
    );
    assert!(
        project(
            ReadTarget::Application(ApplicationId::from_u128(9).unwrap()),
            Record::Application(application())
        )
        .is_err()
    );
    for target in [
        ReadTarget::Client {
            application: ApplicationId::from_u128(9).unwrap(),
            client: client().id,
        },
        ReadTarget::Client {
            application: application().id,
            client: ClientId::from_u128(9).unwrap(),
        },
    ] {
        assert!(project(target, Record::Client(client())).is_err());
    }
    let mut c = client();
    c.spec.resources.clear();
    c.spec.scopes.clear();
    c.spec.refresh_tokens = false;
    let data = project(target(), Record::Client(c)).unwrap();
    assert_eq!(data["resource_ids"], serde_json::json!([]));
    assert_eq!(data["scope_ids"], serde_json::json!([]));
    assert_eq!(data["refresh_tokens"], false);
}
