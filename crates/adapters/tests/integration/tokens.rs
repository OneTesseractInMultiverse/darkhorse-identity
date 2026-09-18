use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use darkhorse_adapters::{
    signing::{configuration::WrapKey, crypto},
    tokens::{
        material::{self, Purpose},
        signer::Signer,
    },
};
use darkhorse_application::{
    oidc::{AuthorizationStore, Decision},
    signing::{SigningStore, WrappedKey},
    tokens::*,
};
use darkhorse_domain::tokens::Error;
#[path = "../unit/signing/fixture.rs"]
mod fixture;
pub(super) const ISSUER: &str = "https://issuer.example";
pub(super) async fn setup() -> (Database, Signer) {
    let db = super::oidc::fixture().await;
    let wrap = WrapKey::from_hex(&"12".repeat(32)).unwrap();
    db.store
        .bind_provider(ISSUER, wrap.fingerprint())
        .await
        .unwrap();
    let key = crypto::import(ISSUER, &wrap, &STANDARD.decode(fixture::PKCS8).unwrap()).unwrap();
    let kid = key.public.kid.clone();
    db.store
        .stage(ISSUER, wrap.fingerprint(), 0, key)
        .await
        .unwrap();
    sqlx::raw_sql("ALTER TABLE signing_keys DISABLE TRIGGER signing_transition; UPDATE signing_keys SET created_ms=created_ms-60001; ALTER TABLE signing_keys ENABLE TRIGGER signing_transition;").execute(&db.pool).await.unwrap();
    db.store.activate(ISSUER, &kid, 1).await.unwrap();
    sqlx::query("INSERT INTO oauth_client_secrets(id,client_id,verifier,created_ms) VALUES('00000000-0000-0000-0000-000000000060','00000000-0000-0000-0000-000000000020',$1,0)").bind([9u8;32].as_slice()).execute(&db.pool).await.unwrap();
    (db, Signer::new(wrap))
}
pub(super) async fn code(db: &Database, handle: [u8; 32]) -> Code {
    db.store
        .begin(super::oidc::request(), handle, Some([1; 32]))
        .await
        .unwrap();
    db.store
        .resume(handle, Some([1; 32]), Decision::Approve)
        .await
        .unwrap();
    db.store
        .issue(
            handle,
            Some([1; 32]),
            material::generate(Purpose::Code).unwrap(),
        )
        .await
        .unwrap()
}
pub(super) fn input(code: &Code) -> Redemption {
    Redemption {
        client: super::oidc::request().client,
        secret: [9; 32],
        code: material::digest(&code.value, Purpose::Code).unwrap(),
        redirect: super::oidc::request().redirect,
        challenge: [7; 32],
    }
}
#[tokio::test]
async fn exchange_is_single_use_and_replay_revokes_the_issued_credential() {
    let (db, signer) = setup().await;
    let code = code(&db, [3; 32]).await;
    assert!(
        db.store
            .issue(
                [3; 32],
                Some([1; 32]),
                material::generate(Purpose::Code).unwrap()
            )
            .await
            .is_err()
    );
    let (a, b) = tokio::join!(
        db.store.redeem(
            input(&code),
            material::generate(Purpose::Access).unwrap(),
            ISSUER,
            &signer
        ),
        db.store.redeem(
            input(&code),
            material::generate(Purpose::Access).unwrap(),
            ISSUER,
            &signer
        )
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let tokens = a.or(b).unwrap();
    assert!(tokens.access.starts_with("da_"));
    assert_eq!(tokens.id_token.split('.').count(), 3);
    assert!(
        db.store
            .userinfo(
                material::digest(&tokens.access, Purpose::Access).unwrap(),
                ISSUER
            )
            .await
            .map(|view| view.subject)
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM access_tokens")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    db.store.close().await;
}
struct Broken;
impl IdSigner for Broken {
    async fn sign(&self, _: WrappedKey, _: IdClaims) -> Result<String, Error> {
        Err(Error::Unavailable)
    }
}
#[tokio::test]
async fn signing_audit_and_wrong_proofs_never_partially_consume_a_code() {
    let (db, signer) = setup().await;
    let code = code(&db, [3; 32]).await;
    for n in 0..4 {
        let mut proof = input(&code);
        match n {
            0 => proof.secret = [0; 32],
            1 => proof.challenge = [0; 32],
            2 => proof.redirect.push('/'),
            _ => proof.client = darkhorse_domain::identity::ClientId::from_u128(99).unwrap(),
        };
        assert!(
            db.store
                .redeem(
                    proof,
                    material::generate(Purpose::Access).unwrap(),
                    ISSUER,
                    &signer
                )
                .await
                .is_err()
        );
    }
    assert!(matches!(
        db.store
            .redeem(
                input(&code),
                material::generate(Purpose::Access).unwrap(),
                ISSUER,
                &Broken
            )
            .await,
        Err(Error::Unavailable)
    ));
    sqlx::raw_sql("CREATE FUNCTION reject_token_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected audit failure'; END $$; CREATE TRIGGER reject_token_audit BEFORE INSERT ON token_audit FOR EACH ROW EXECUTE FUNCTION reject_token_audit();").execute(&db.pool).await.unwrap();
    assert!(
        db.store
            .redeem(
                input(&code),
                material::generate(Purpose::Access).unwrap(),
                ISSUER,
                &signer
            )
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM access_tokens")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query("DROP TRIGGER reject_token_audit ON token_audit")
        .execute(&db.pool)
        .await
        .unwrap();
    let token = db
        .store
        .redeem(
            input(&code),
            material::generate(Purpose::Access).unwrap(),
            ISSUER,
            &signer,
        )
        .await
        .unwrap();
    let digest = material::digest(&token.access, Purpose::Access).unwrap();
    assert_eq!(
        db.store
            .userinfo(digest, ISSUER)
            .await
            .map(|view| view.subject),
        Ok(id(1))
    );
    use darkhorse_application::authentication::AuthenticationStore;
    db.store.logout([1; 32]).await.unwrap();
    assert_eq!(
        db.store
            .userinfo(digest, ISSUER)
            .await
            .map(|view| view.subject),
        Err(Error::InvalidToken)
    );
    db.store.close().await;
}

#[tokio::test]
async fn committed_registration_and_account_changes_reject_both_code_and_access() {
    for statement in [
        "UPDATE oauth_clients SET revision=revision+1",
        "UPDATE oauth_clients SET revision=revision+1,active=false",
        "UPDATE applications SET revision=revision+1,active=false",
        "UPDATE principals SET credential_epoch=credential_epoch+1,revision=revision+1",
        "UPDATE principals SET active=false,credential_epoch=credential_epoch+1,revision=revision+1 WHERE id='00000000-0000-0000-0000-000000000001'",
        "UPDATE credentials SET revoked=true WHERE principal_id='00000000-0000-0000-0000-000000000001'",
        "UPDATE browser_sessions SET revoked=true",
    ] {
        let (db, signer) = setup().await;
        insert_principal(&db, 2, true).await;
        let first = code(&db, [3; 32]).await;
        let second = code(&db, [4; 32]).await;
        let issued = db
            .store
            .redeem(
                input(&first),
                material::generate(Purpose::Access).unwrap(),
                ISSUER,
                &signer,
            )
            .await
            .unwrap();
        sqlx::query(statement).execute(&db.pool).await.unwrap();
        assert!(
            db.store
                .redeem(
                    input(&second),
                    material::generate(Purpose::Access).unwrap(),
                    ISSUER,
                    &signer
                )
                .await
                .is_err()
        );
        assert_eq!(
            db.store
                .userinfo(
                    material::digest(&issued.access, Purpose::Access).unwrap(),
                    ISSUER
                )
                .await
                .map(|view| view.subject),
            Err(Error::InvalidToken)
        );
        db.store.close().await;
    }
}
#[tokio::test]
async fn expired_fresh_codes_deny_and_expired_replays_still_revoke_access() {
    let (db, signer) = setup().await;
    let first = code(&db, [3; 32]).await;
    let second = code(&db, [4; 32]).await;
    let issued = db
        .store
        .redeem(
            input(&first),
            material::generate(Purpose::Access).unwrap(),
            ISSUER,
            &signer,
        )
        .await
        .unwrap();
    // Fixture clock advancement is restricted to this disposable owner connection.
    sqlx::raw_sql("ALTER TABLE authorization_codes DISABLE TRIGGER code_transition; UPDATE authorization_codes SET created_ms=created_ms-60001,expires_ms=expires_ms-60001; ALTER TABLE authorization_codes ENABLE TRIGGER code_transition;").execute(&db.pool).await.unwrap();
    for attempt in [&first, &second] {
        assert!(matches!(
            db.store
                .redeem(
                    input(attempt),
                    material::generate(Purpose::Access).unwrap(),
                    ISSUER,
                    &signer
                )
                .await,
            Err(Error::InvalidGrant)
        ));
    }
    assert_eq!(
        db.store
            .userinfo(
                material::digest(&issued.access, Purpose::Access).unwrap(),
                ISSUER
            )
            .await
            .map(|view| view.subject),
        Err(Error::InvalidToken)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM access_tokens")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    db.store.close().await;
}
#[tokio::test]
async fn issuance_failure_and_constraints_preserve_pending_request_and_grant_ceiling() {
    let (db, signer) = setup().await;
    db.store
        .begin(super::oidc::request(), [3; 32], Some([1; 32]))
        .await
        .unwrap();
    db.store
        .resume([3; 32], Some([1; 32]), Decision::Approve)
        .await
        .unwrap();
    assert!(matches!(
        db.store
            .issue([3; 32], None, material::generate(Purpose::Code).unwrap())
            .await,
        Err(Error::InvalidGrant)
    ));
    sqlx::raw_sql("CREATE FUNCTION reject_code_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected audit failure'; END $$; CREATE TRIGGER reject_code_audit BEFORE INSERT ON token_audit FOR EACH ROW EXECUTE FUNCTION reject_code_audit();").execute(&db.pool).await.unwrap();
    assert!(matches!(
        db.store
            .issue(
                [3; 32],
                Some([1; 32]),
                material::generate(Purpose::Code).unwrap()
            )
            .await,
        Err(Error::Unavailable)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM authorization_codes")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query("DROP TRIGGER reject_code_audit ON token_audit")
        .execute(&db.pool)
        .await
        .unwrap();
    let issued_code = db
        .store
        .issue(
            [3; 32],
            Some([1; 32]),
            material::generate(Purpose::Code).unwrap(),
        )
        .await
        .unwrap();
    let issued = db
        .store
        .redeem(
            input(&issued_code),
            material::generate(Purpose::Access).unwrap(),
            ISSUER,
            &signer,
        )
        .await
        .unwrap();
    for statement in [
        "UPDATE authorization_codes SET nonce='replacement'",
        "UPDATE authorization_codes SET consumed=false",
        "UPDATE access_tokens SET audience='urn:other'",
        "UPDATE access_tokens SET claim_ceiling=ARRAY['email']",
        "UPDATE access_tokens SET capability_ceiling=ARRAY['00000000-0000-0000-0000-000000000099']::uuid[]",
        "DELETE FROM token_audit",
    ] {
        assert!(sqlx::query(statement).execute(&db.pool).await.is_err());
    }
    let digest = material::digest(&issued.access, Purpose::Access).unwrap();
    assert_eq!(
        db.store
            .userinfo(digest, "https://other.example")
            .await
            .map(|view| view.subject),
        Err(Error::InvalidToken)
    );
    assert_eq!(
        db.store
            .userinfo(digest, ISSUER)
            .await
            .map(|view| view.subject),
        Ok(id(1))
    );
    sqlx::query("UPDATE access_tokens SET revoked=true")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(
        sqlx::query("UPDATE access_tokens SET revoked=false")
            .execute(&db.pool)
            .await
            .is_err()
    );
    db.store.close().await;
}

#[tokio::test]
async fn discovery_does_not_serialize_with_signing_readers() {
    let (db, _) = setup().await;
    let mut tx = db.pool.begin().await.unwrap();
    sqlx::query("SELECT singleton FROM provider_state FOR SHARE")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        db.store.inventory(ISSUER),
    )
    .await;
    tx.rollback().await.unwrap();
    assert!(
        result.is_ok(),
        "public metadata must coexist with active signing readers"
    );
    assert_eq!(result.unwrap().unwrap().keys.len(), 1);
    db.store.close().await;
}
