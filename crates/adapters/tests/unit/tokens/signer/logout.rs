use super::*;
use darkhorse_application::logout::{LogoutClaims, LogoutSigner};
use darkhorse_domain::identity::LogoutTokenId;

fn logout_claims() -> LogoutClaims {
    LogoutClaims {
        issuer: "https://issuer.example".into(),
        client: ClientId::from_u128(1).unwrap(),
        subject: PrincipalId::from_u128(2).unwrap(),
        session: RelyingPartySessionId::from_u128(3).unwrap(),
        token: LogoutTokenId::from_u128(4).unwrap(),
        issued: 1001,
    }
}

fn key(wrap: &WrapKey) -> WrappedKey {
    use base64::engine::general_purpose::STANDARD;
    crypto::import(
        "https://issuer.example",
        wrap,
        &STANDARD.decode(fixture::PKCS8).unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn logout_signature_and_claims_bind_only_the_selected_client_subject_and_session() {
    let wrap = WrapKey::from_hex(&"12".repeat(32)).unwrap();
    let key = key(&wrap);
    let public = key.public.clone();
    let token = Signer::new(wrap)
        .sign_logout(key, logout_claims())
        .await
        .unwrap();
    let parts: Vec<_> = token.split('.').collect();
    assert_eq!(parts.len(), 3);
    let n = URL_SAFE_NO_PAD.decode(&public.n).unwrap();
    let e = URL_SAFE_NO_PAD.decode(&public.e).unwrap();
    ring::signature::RsaPublicKeyComponents { n, e }
        .verify(
            &ring::signature::RSA_PKCS1_2048_8192_SHA256,
            parts[..2].join(".").as_bytes(),
            &URL_SAFE_NO_PAD.decode(parts[2]).unwrap(),
        )
        .unwrap();
    let header: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
    let claims: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
    assert_eq!(
        header,
        serde_json::json!({"alg":"RS256", "typ":"logout+jwt", "kid":public.kid})
    );
    assert_eq!(
        claims,
        serde_json::json!({
            "iss":"https://issuer.example", "aud":"00000000-0000-0000-0000-000000000001",
            "sub":"00000000-0000-0000-0000-000000000002", "sid":"00000000-0000-0000-0000-000000000003",
            "jti":"00000000-0000-0000-0000-000000000004", "iat":1001, "exp":1121,
            "events":{"http://schemas.openid.net/event/backchannel-logout":{}}
        })
    );
}

#[tokio::test]
async fn logout_signing_rejects_time_overflow_wrong_issuer_and_corrupt_keys() {
    let wrap = WrapKey::from_hex(&"12".repeat(32)).unwrap();
    let key = key(&wrap);
    let signer = Signer::new(wrap);
    let mut input = logout_claims();
    input.issued = u64::MAX;
    assert_eq!(
        signer.sign_logout(copy(&key), input).await,
        Err(Error::Unavailable)
    );
    let mut input = logout_claims();
    input.issuer = "https://other.example".into();
    assert_eq!(
        signer.sign_logout(copy(&key), input).await,
        Err(Error::Unavailable)
    );
    let mut damaged = copy(&key);
    damaged.ciphertext[0] ^= 1;
    assert_eq!(
        signer.sign_logout(damaged, logout_claims()).await,
        Err(Error::Unavailable)
    );
    let mut damaged = copy(&key);
    damaged.public.kid.push('x');
    assert_eq!(
        signer.sign_logout(damaged, logout_claims()).await,
        Err(Error::Unavailable)
    );
    assert!(signer.sign_logout(key, logout_claims()).await.is_ok());
}

#[tokio::test]
async fn logout_and_id_tokens_share_one_fail_fast_signing_budget() {
    let wrap = WrapKey::from_hex(&"12".repeat(32)).unwrap();
    let key = key(&wrap);
    let signer = Signer::new(wrap);
    let permit = signer.slots.acquire_many(4).await.unwrap();
    assert_eq!(
        signer.sign_logout(copy(&key), logout_claims()).await,
        Err(Error::Unavailable)
    );
    assert_eq!(
        signer.sign(copy(&key), claims()).await,
        Err(Error::Unavailable)
    );
    drop(permit);
    assert!(
        signer
            .sign_logout(copy(&key), logout_claims())
            .await
            .is_ok()
    );
    assert!(signer.sign(key, claims()).await.is_ok());
}

#[test]
fn cancelling_a_waiter_keeps_queued_signing_work_inside_the_shared_budget() {
    use std::{sync::mpsc, time::Duration};
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    let wrap = WrapKey::from_hex(&"12".repeat(32)).unwrap();
    let key = key(&wrap);
    let signer = Arc::new(Signer::new(wrap));
    let (release, blocked) = mpsc::sync_channel(1);
    let (started, ready) = tokio::sync::oneshot::channel();
    let blocker = runtime.spawn_blocking(move || {
        started.send(()).unwrap();
        blocked.recv_timeout(Duration::from_secs(10)).unwrap();
    });
    runtime.block_on(async {
        ready.await.unwrap();
        let _occupied = signer.slots.acquire_many(3).await.unwrap();
        let worker = signer.clone();
        let work_key = copy(&key);
        let task = tokio::spawn(async move { worker.sign_logout(work_key, logout_claims()).await });
        tokio::time::timeout(Duration::from_secs(5), async {
            while signer.slots.available_permits() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_eq!(signer.slots.available_permits(), 0);
        assert_eq!(signer.sign(key, claims()).await, Err(Error::Unavailable));
        release.send(()).unwrap();
        blocker.await.unwrap();
        let permit = tokio::time::timeout(Duration::from_secs(5), signer.slots.acquire())
            .await
            .expect("finished signing releases its permit")
            .unwrap();
        drop(permit);
    });
}
