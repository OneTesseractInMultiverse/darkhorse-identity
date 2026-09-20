use super::*;
use crate::signing_fixture as fixture;
use darkhorse_domain::identity::{ClientId, PrincipalId, RelyingPartySessionId};
fn claims() -> IdClaims {
    IdClaims {
        issuer: "https://issuer.example".into(),
        client: ClientId::from_u128(1).unwrap(),
        subject: PrincipalId::from_u128(2).unwrap(),
        session: RelyingPartySessionId::from_u128(3).unwrap(),
        nonce: Some("test-nonce".into()),
        authenticated: 1000,
        issued: 1001,
        expires: 1301,
    }
}
#[test]
fn claim_schema_binds_nonce_subject_and_authentication_event_without_logout_claims() {
    let encoded = message("public-kid", &claims()).unwrap();
    let parts: Vec<serde_json::Value> = encoded
        .split('.')
        .map(|p| serde_json::from_slice(&URL_SAFE_NO_PAD.decode(p).unwrap()).unwrap())
        .collect();
    assert_eq!(parts[0]["typ"], "JWT");
    assert_eq!(parts[1]["nonce"], "test-nonce");
    assert_eq!(parts[1]["auth_time"], 1000);
    assert_eq!(parts[1]["sub"], "00000000-0000-0000-0000-000000000002");
    assert!(
        parts[1]["sid"].is_string(),
        "ID tokens identify the RP session"
    );
    assert_eq!(parts[1]["sid"], "00000000-0000-0000-0000-000000000003");
    assert!(parts[1].get("events").is_none());
    let mut input = claims();
    input.nonce = None;
    assert!(!message("public-kid", &input).unwrap().is_empty());
    input.authenticated = 1002;
    assert_eq!(message("public-kid", &input), Err(Error::Unavailable));
    input.authenticated = 1000;
    input.expires = 1302;
    assert_eq!(message("public-kid", &input), Err(Error::Unavailable));
}
#[tokio::test]
async fn signing_rejects_a_different_issuer_and_exhausted_worker_budget() {
    use base64::engine::general_purpose::STANDARD;
    let wrap = WrapKey::from_hex(&"12".repeat(32)).unwrap();
    let key = crypto::import(
        "https://issuer.example",
        &wrap,
        &STANDARD.decode(fixture::PKCS8).unwrap(),
    )
    .unwrap();
    let signer = Signer::new(wrap);
    let mut input = claims();
    input.issuer = "https://other.example".into();
    assert_eq!(
        signer.sign(copy(&key), input).await,
        Err(Error::Unavailable)
    );
    let permit = signer.slots.acquire_many(4).await.unwrap();
    assert_eq!(
        signer.sign(copy(&key), claims()).await,
        Err(Error::Unavailable)
    );
    drop(permit);
    assert_eq!(
        signer.sign(key, claims()).await.unwrap().split('.').count(),
        3
    );
}

fn copy(key: &WrappedKey) -> WrappedKey {
    WrappedKey {
        public: key.public.clone(),
        nonce: key.nonce,
        ciphertext: key.ciphertext.clone(),
    }
}
