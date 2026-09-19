use super::tokens::{ISSUER, code, input, setup};
use super::*;
use darkhorse_adapters::tokens::material::{self, Purpose};
use darkhorse_application::{
    oidc::{AuthorizationStore, Decision},
    tokens::*,
};
use darkhorse_domain::{identity::ClientId, tokens::Error};
fn request(token: Option<[u8; 32]>) -> Management {
    Management {
        client: ClientId::from_u128(32).unwrap(),
        secret: [9; 32],
        token: token.map(ManagedToken::Access),
    }
}
async fn issue(db: &Database, signer: &impl IdSigner, scopes: &[&str], handle: [u8; 32]) -> Tokens {
    let mut authorization = super::oidc::request();
    authorization.scopes = scopes.iter().map(|s| (*s).into()).collect();
    db.store
        .begin(authorization, handle, Some([1; 32]))
        .await
        .unwrap();
    db.store
        .resume(handle, Some([1; 32]), Decision::Approve)
        .await
        .unwrap();
    let code = db
        .store
        .issue(
            handle,
            Some([1; 32]),
            material::generate(Purpose::Code).unwrap(),
        )
        .await
        .unwrap();
    db.store
        .redeem(input(&code), material::pair().unwrap(), ISSUER, signer)
        .await
        .unwrap()
}
#[tokio::test]
async fn approved_profile_claims_are_minimal_and_reduction_invalidates_old_tokens() {
    let (db, signer) = setup().await;
    let original = issue(&db, &signer, &["openid"], [3; 32]).await;
    let original_digest = material::digest(&original.access, Purpose::Access).unwrap();
    let expanded = issue(&db, &signer, &["openid", "profile", "email"], [4; 32]).await;
    let expanded_digest = material::digest(&expanded.access, Purpose::Access).unwrap();
    let minimal = db.store.userinfo(original_digest, ISSUER).await.unwrap();
    assert!(minimal.profile.is_none() && minimal.email.is_none());
    let full = db.store.userinfo(expanded_digest, ISSUER).await.unwrap();
    assert_eq!(full.subject, id(1));
    assert_eq!(full.profile.unwrap().given, "Ada");
    assert_eq!(full.email.as_deref(), Some("one@example.com"));
    let audit_before: i64 = sqlx::query_scalar("SELECT count(*) FROM token_audit")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    for _ in 0..3 {
        let active = db
            .store
            .introspect(request(Some(expanded_digest)), ISSUER)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(active.scope, "openid profile email");
        assert_eq!(active.subject, id(1));
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM token_audit")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        audit_before
    );
    sqlx::query("UPDATE oauth_consents SET scopes=ARRAY['openid']")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(
        db.store
            .introspect(request(Some(expanded_digest)), ISSUER)
            .await
            .unwrap()
            .is_none()
    );
    assert!(db.store.userinfo(expanded_digest, ISSUER).await.is_err());
    assert!(
        db.store
            .introspect(request(Some(original_digest)), ISSUER)
            .await
            .unwrap()
            .is_some()
    );
    db.store.close().await;
}
#[tokio::test]
async fn revocation_is_atomic_idempotent_and_never_crosses_the_client_boundary() {
    let (db, signer) = setup().await;
    let issued = issue(&db, &signer, &["openid"], [3; 32]).await;
    let digest = material::digest(&issued.access, Purpose::Access).unwrap();
    sqlx::raw_sql("INSERT INTO oauth_clients(id,application_id,name,active) VALUES('00000000-0000-0000-0000-000000000021','00000000-0000-0000-0000-000000000010','Other',true);").execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO oauth_client_secrets(id,client_id,verifier,created_ms) VALUES('00000000-0000-0000-0000-000000000061','00000000-0000-0000-0000-000000000021',$1,0)").bind([8u8;32].as_slice()).execute(&db.pool).await.unwrap();
    let foreign = || Management {
        client: ClientId::from_u128(33).unwrap(),
        secret: [8; 32],
        token: Some(ManagedToken::Access(digest)),
    };
    assert!(
        db.store
            .introspect(foreign(), ISSUER)
            .await
            .unwrap()
            .is_none()
    );
    db.store.revoke(foreign(), ISSUER).await.unwrap();
    assert!(
        db.store
            .introspect(request(Some(digest)), ISSUER)
            .await
            .unwrap()
            .is_some()
    );
    for token in [None, Some([0; 32])] {
        assert!(
            db.store
                .introspect(request(token), ISSUER)
                .await
                .unwrap()
                .is_none()
        );
        db.store.revoke(request(token), ISSUER).await.unwrap();
        let mut bad = request(token);
        bad.secret = [0; 32];
        assert!(matches!(
            db.store.introspect(bad, ISSUER).await,
            Err(Error::InvalidClient)
        ));
    }
    sqlx::raw_sql("CREATE FUNCTION reject_revocation_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected audit failure'; END $$; CREATE TRIGGER reject_revocation_audit BEFORE INSERT ON token_audit FOR EACH ROW EXECUTE FUNCTION reject_revocation_audit();").execute(&db.pool).await.unwrap();
    assert_eq!(
        db.store.revoke(request(Some(digest)), ISSUER).await,
        Err(Error::Unavailable)
    );
    assert!(
        db.store
            .introspect(request(Some(digest)), ISSUER)
            .await
            .unwrap()
            .is_some()
    );
    sqlx::query("DROP TRIGGER reject_revocation_audit ON token_audit")
        .execute(&db.pool)
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        db.store.revoke(request(Some(digest)), ISSUER),
        db.store.revoke(request(Some(digest)), ISSUER)
    );
    assert_eq!(a, Ok(()));
    assert_eq!(b, Ok(()));
    assert!(
        db.store
            .introspect(request(Some(digest)), ISSUER)
            .await
            .unwrap()
            .is_none()
    );
    assert!(db.store.userinfo(digest, ISSUER).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM token_audit WHERE event='access_revoked'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        1
    );
    db.store.close().await;
}
#[tokio::test]
async fn unknown_issuer_expired_or_unavailable_authority_never_becomes_active() {
    let (db, signer) = setup().await;
    let code = code(&db, [3; 32]).await;
    let token = db
        .store
        .redeem(input(&code), material::pair().unwrap(), ISSUER, &signer)
        .await
        .unwrap();
    let digest = material::digest(&token.access, Purpose::Access).unwrap();
    assert!(
        db.store
            .introspect(request(Some(digest)), "https://other.example")
            .await
            .unwrap()
            .is_none()
    );
    sqlx::raw_sql("ALTER TABLE access_tokens DISABLE TRIGGER access_transition; UPDATE access_tokens SET created_ms=created_ms-300001,expires_ms=expires_ms-300001; ALTER TABLE access_tokens ENABLE TRIGGER access_transition;").execute(&db.pool).await.unwrap();
    assert!(
        db.store
            .introspect(request(Some(digest)), ISSUER)
            .await
            .unwrap()
            .is_none()
    );
    db.store.close().await;
    assert!(matches!(
        db.store.introspect(request(Some(digest)), ISSUER).await,
        Err(Error::Unavailable)
    ));
    assert_eq!(
        db.store.revoke(request(Some(digest)), ISSUER).await,
        Err(Error::Unavailable)
    );
}

#[tokio::test]
async fn readers_wait_for_in_flight_revocation_and_new_checks_observe_commit() {
    let (db, signer) = setup().await;
    let token = issue(&db, &signer, &["openid"], [3; 32]).await;
    let digest = material::digest(&token.access, Purpose::Access).unwrap();
    let mut writer = db.pool.begin().await.unwrap();
    sqlx::query("SELECT singleton FROM security_state FOR SHARE")
        .fetch_one(&mut *writer)
        .await
        .unwrap();
    sqlx::query("SELECT c.digest FROM authorization_codes c JOIN access_tokens t ON t.code_digest=c.digest WHERE t.digest=$1 FOR UPDATE OF c").bind(digest.as_slice()).fetch_one(&mut *writer).await.unwrap();
    sqlx::query("UPDATE access_tokens SET revoked=true WHERE digest=$1")
        .bind(digest.as_slice())
        .execute(&mut *writer)
        .await
        .unwrap();
    let replica = db.store.clone();
    let check =
        tokio::spawn(async move { replica.introspect(request(Some(digest)), ISSUER).await });
    let mut waiting = false;
    for _ in 0..100 {
        waiting=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'SELECT * FROM authorization_codes%')").fetch_one(&db.pool).await.unwrap();
        if waiting {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(waiting, "reader must wait on the code lock");
    writer.commit().await.unwrap();
    assert!(check.await.unwrap().unwrap().is_none());
    for _ in 0..5 {
        assert!(
            db.store
                .introspect(request(Some(digest)), ISSUER)
                .await
                .unwrap()
                .is_none()
        );
    }
    db.store.close().await;
}
