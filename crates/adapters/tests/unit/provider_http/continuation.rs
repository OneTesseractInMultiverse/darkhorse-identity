use super::*;
use darkhorse_domain::{
    identity::ClientId,
    localization::Locale,
    oidc::{Prompt, Request},
};

fn pending() -> Preauthorization {
    Preauthorization {
        request: Request {
            client: ClientId::from_u128(1).unwrap(),
            redirect: "https://client.example/callback".into(),
            challenge: [8; 32],
            state: Some("state&x=1".into()),
            nonce: Some("nonce".into()),
            scopes: vec!["openid".into(), "email".into()],
            resource: None,
            prompt: Prompt::LoginConsent,
            max_age: Some(0),
            ui_locale: Some(Locale::Spanish),
        },
        created_ms: 900,
        initial_session: Some([3; 32]),
        client_revision: 4,
        application_revision: 5,
        resource_policy_revision: None,
    }
}
#[test]
fn envelope_retains_all_original_facts_and_uses_fresh_nonces() {
    let key = ContinuationKey::derive(&[7; 32]).unwrap();
    let expected = wire::encode(pending()).unwrap();
    let one = key
        .seal(pending(), "https://issuer.example", &[1; 32])
        .unwrap();
    let two = key
        .seal(pending(), "https://issuer.example", &[1; 32])
        .unwrap();
    assert_ne!(one, two);
    assert!(!one.contains("client.example"));
    let reopened = ContinuationKey::derive(&[7; 32])
        .unwrap()
        .open(&one, "https://issuer.example", &[1; 32])
        .unwrap();
    assert_eq!(wire::encode(reopened).unwrap(), expected);
}
#[test]
fn envelope_rejects_wrong_key_issuer_handle_and_modified_or_oversized_bytes() {
    let key = ContinuationKey::derive(&[7; 32]).unwrap();
    let encoded = key
        .seal(pending(), "https://issuer.example", &[1; 32])
        .unwrap();
    assert!(
        key.open(&encoded, "https://other.example", &[1; 32])
            .is_err()
    );
    assert!(
        key.open(&encoded, "https://issuer.example", &[2; 32])
            .is_err()
    );
    assert!(
        ContinuationKey::derive(&[8; 32])
            .unwrap()
            .open(&encoded, "https://issuer.example", &[1; 32])
            .is_err()
    );
    let mut bytes = URL_SAFE_NO_PAD.decode(&encoded).unwrap();
    for position in [0, 12, bytes.len() - 1] {
        bytes[position] ^= 1;
        assert!(
            key.open(
                &URL_SAFE_NO_PAD.encode(&bytes),
                "https://issuer.example",
                &[1; 32]
            )
            .is_err()
        );
        bytes[position] ^= 1;
    }
    for invalid in [
        "".into(),
        "!".repeat(40),
        "A".repeat(37),
        "A".repeat(MAX_REFERENCE_BYTES),
    ] {
        assert!(
            key.open(&invalid, "https://issuer.example", &[1; 32])
                .is_err()
        );
    }
}
#[test]
fn maximum_domain_fields_fit_the_envelope_without_weakening_validation() {
    let mut value = pending();
    value.request.redirect = "\\".repeat(2048);
    value.request.state = Some("\"".repeat(512));
    value.request.nonce = Some("\\".repeat(256));
    value.request.resource = Some("\\".repeat(256));
    value.request.scopes = (0..32)
        .map(|n| format!("{n:02}{}", "!".repeat(98)))
        .collect();
    value.resource_policy_revision = Some(6);
    let key = ContinuationKey::derive(&[7; 32]).unwrap();
    let expected = wire::encode(value.clone()).unwrap();
    let encoded = key.seal(value, "https://issuer.example", &[1; 32]).unwrap();
    assert!(encoded.len() + 65 <= MAX_REFERENCE_BYTES);
    assert_eq!(
        wire::encode(
            key.open(&encoded, "https://issuer.example", &[1; 32])
                .unwrap()
        )
        .unwrap(),
        expected
    );
    let mut invalid = pending();
    invalid.request.scopes.clear();
    assert!(
        key.seal(invalid, "https://issuer.example", &[1; 32])
            .is_err()
    );
}
