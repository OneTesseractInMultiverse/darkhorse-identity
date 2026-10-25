use super::tokens::{ISSUER, code, input, setup};
use super::*;
use darkhorse_adapters::tokens::material;
use darkhorse_application::{
    authentication::AuthenticationStore,
    logout::{delivery::Queue, registration::*},
    tokens::TokenStore,
};
use darkhorse_domain::{
    identity::{ApplicationId, ClientId},
    logout::DeliveryResult,
    registration::RegistrationError,
};

fn target() -> Target {
    Target {
        application: ApplicationId::from_u128(16).unwrap(),
        client: ClientId::from_u128(32).unwrap(),
    }
}
fn config(revision: u64, uri: Option<&str>) -> Configuration {
    Configuration {
        revision,
        uri: uri.map(str::to_owned),
    }
}
async fn configured() -> Database {
    let (db, signer) = setup().await;
    db.store
        .configure_logout(
            [1; 32],
            target(),
            config(0, Some("https://client.example/logout")),
        )
        .await
        .unwrap();
    let code = code(&db, [3; 32]).await;
    db.store
        .redeem(input(&code), material::pair().unwrap(), ISSUER, &signer)
        .await
        .unwrap();
    db
}
async fn state(db: &Database) -> String {
    sqlx::query_scalar("SELECT state FROM logout_deliveries")
        .fetch_one(&db.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn logout_registration_requires_current_recent_admin_and_revision_with_immutable_audit() {
    let (db, _) = setup().await;
    assert_eq!(
        db.store
            .logout_configuration([1; 32], target())
            .await
            .unwrap()
            .configuration
            .revision,
        0
    );
    assert!(matches!(
        db.store
            .configure_logout([9; 32], target(), config(0, None))
            .await,
        Err(RegistrationError::Unauthorized)
    ));
    assert!(matches!(
        db.store
            .configure_logout([1; 32], target(), config(0, Some("http://bad.example/")))
            .await,
        Err(RegistrationError::Invalid)
    ));
    db.store
        .configure_logout(
            [1; 32],
            target(),
            config(0, Some("https://client.example/logout")),
        )
        .await
        .unwrap();
    assert!(matches!(
        db.store
            .configure_logout([1; 32], target(), config(0, None))
            .await,
        Err(RegistrationError::Conflict)
    ));
    assert!(
        sqlx::query("UPDATE logout_destinations SET uri='https://evil.example/'")
            .execute(&db.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM logout_audit")
            .execute(&db.pool)
            .await
            .is_err()
    );
    sqlx::raw_sql("ALTER TABLE browser_sessions DISABLE TRIGGER browser_session_transition; UPDATE browser_sessions SET created_ms=created_ms-300001,expires_ms=expires_ms-300001; ALTER TABLE browser_sessions ENABLE TRIGGER browser_session_transition;")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(matches!(
        db.store
            .configure_logout([1; 32], target(), config(1, None))
            .await,
        Err(RegistrationError::RecentAuthenticationRequired)
    ));
    assert!(
        db.store
            .logout_configuration([1; 32], target())
            .await
            .is_ok()
    );
    db.store.close().await;
}

#[tokio::test]
async fn logout_and_intent_commit_together_and_signout_remains_idempotent() {
    let db = configured().await;
    sqlx::raw_sql("CREATE FUNCTION reject_logout_intent() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected'; END $$; CREATE TRIGGER reject_logout_intent BEFORE INSERT ON logout_intents FOR EACH ROW EXECUTE FUNCTION reject_logout_intent();").execute(&db.pool).await.unwrap();
    assert!(db.store.logout([1; 32]).await.is_err());
    assert!(db.store.session([1; 32]).await.is_ok());
    sqlx::query("DROP TRIGGER reject_logout_intent ON logout_intents")
        .execute(&db.pool)
        .await
        .unwrap();
    db.store.logout([1; 32]).await.unwrap();
    db.store.logout([1; 32]).await.unwrap();
    assert!(db.store.session([1; 32]).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM logout_intents")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM logout_deliveries")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    let delivery = db.store.claim().await.unwrap().unwrap();
    assert_eq!(delivery.destination, "https://client.example/logout");
    assert_eq!(delivery.claims.client, target().client);
    assert_eq!(delivery.claims.issuer, ISSUER);
    assert_eq!(
        delivery.claims.session.as_u128(),
        sqlx::query_scalar::<_, Uuid>("SELECT sid FROM relying_party_sessions")
            .fetch_one(&db.pool)
            .await
            .unwrap()
            .as_u128()
    );
    db.store
        .finish(delivery.id, delivery.attempt, DeliveryResult::Accepted)
        .await
        .unwrap();
    assert_eq!(state(&db).await, "delivered");
    assert!(db.store.claim().await.unwrap().is_none());
    db.store.close().await;
}

#[tokio::test]
async fn logout_claims_are_exclusive_and_recover_with_fresh_tokens_and_fenced_completion() {
    let db = configured().await;
    db.store.logout([1; 32]).await.unwrap();
    let independent = PostgresStore::from_pool(
        PgPool::connect_with((*db.pool.connect_options()).clone())
            .await
            .unwrap(),
    );
    let (a, b) = tokio::join!(db.store.claim(), independent.claim());
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(usize::from(a.is_some()) + usize::from(b.is_some()), 1);
    let first = a.or(b).unwrap();
    sqlx::query("UPDATE logout_deliveries SET next_ms=created_ms")
        .execute(&db.pool)
        .await
        .unwrap();
    let second = independent.claim().await.unwrap().unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(second.attempt, 2);
    assert_ne!(first.claims.token, second.claims.token);
    db.store
        .finish(first.id, first.attempt, DeliveryResult::Accepted)
        .await
        .unwrap();
    assert_eq!(state(&db).await, "queued");
    db.store
        .finish(second.id, second.attempt, DeliveryResult::Retry)
        .await
        .unwrap();
    assert!(db.store.claim().await.unwrap().is_none());
    sqlx::query("UPDATE logout_deliveries SET next_ms=created_ms,attempts=8")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(db.store.claim().await.unwrap().is_none());
    assert_eq!(state(&db).await, "failed");
    independent.close().await;
    db.store.close().await;
}

#[tokio::test]
async fn logout_destination_changes_never_retarget_notifications_even_before_expansion() {
    let db = configured().await;
    let candidate = db
        .store
        .candidate("one@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [2; 32], None).await.unwrap();
    db.store.logout([1; 32]).await.unwrap();
    db.store
        .configure_logout(
            [2; 32],
            target(),
            config(1, Some("https://new.example/logout")),
        )
        .await
        .unwrap();
    assert!(db.store.claim().await.unwrap().is_none());
    assert_eq!(state(&db).await, "cancelled");
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM logout_deliveries")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(revision, 1);
    let status = db
        .store
        .logout_configuration([2; 32], target())
        .await
        .unwrap();
    assert_eq!(status.configuration.revision, 2);
    assert_eq!(status.deliveries[0].state, "cancelled");
    db.store.close().await;
}

#[tokio::test]
async fn logout_missing_signing_key_does_not_lose_intent_and_expired_work_never_sends() {
    let db = configured().await;
    db.store.logout([1; 32]).await.unwrap();
    sqlx::raw_sql("ALTER TABLE signing_keys DISABLE TRIGGER signing_transition; UPDATE signing_keys SET phase='retiring',verify_until_ms=activated_ms+600000; ALTER TABLE signing_keys ENABLE TRIGGER signing_transition;").execute(&db.pool).await.unwrap();
    assert!(db.store.claim().await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM logout_intents WHERE NOT expanded")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    sqlx::query("UPDATE logout_intents SET created_ms=created_ms-86400001")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(db.store.claim().await.unwrap().is_none());
    assert_eq!(state(&db).await, "failed");
    db.store.close().await;
}

#[tokio::test]
async fn logout_upgrade_preserves_existing_sessions_and_starts_with_no_targets() {
    let db = Database::at_version(41).await;
    db.store
        .bootstrap(administrator(1, "one@example.com"))
        .await
        .unwrap();
    let candidate = db
        .store
        .candidate("one@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [1; 32], None).await.unwrap();
    db.store.migrate().await.unwrap();
    db.store.migrate().await.unwrap();
    assert!(db.store.session([1; 32]).await.is_ok());
    assert!(db.store.claim().await.unwrap().is_none());
    db.store.close().await;
}

#[tokio::test]
async fn logout_large_fanout_is_paged_without_losing_or_retargeting_associations() {
    let db = configured().await;
    sqlx::raw_sql("INSERT INTO oauth_clients(id,application_id,name,active) SELECT lpad(to_hex(n),32,'0')::uuid,'00000000-0000-0000-0000-000000000010','Client '||n,true FROM generate_series(1000,1100) n; INSERT INTO logout_destinations(client_id,revision,uri) SELECT id,1,'https://client.example/logout' FROM oauth_clients WHERE name LIKE 'Client %'; INSERT INTO relying_party_sessions(issuer,session_id,principal_id,client_id,created_ms) SELECT r.issuer,r.session_id,r.principal_id,c.id,r.created_ms FROM relying_party_sessions r CROSS JOIN oauth_clients c WHERE c.name LIKE 'Client %';").execute(&db.pool).await.unwrap();
    db.store.logout([1; 32]).await.unwrap();
    assert!(db.store.claim().await.unwrap().is_some());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM logout_deliveries")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        100
    );
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT expanded FROM logout_intents")
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
    assert!(db.store.claim().await.unwrap().is_some());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM logout_deliveries")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        102
    );
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT expanded FROM logout_intents")
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
    let audits: i64 = sqlx::query_scalar("SELECT count(*) FROM logout_audit")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    sqlx::raw_sql("UPDATE logout_deliveries SET state='failed',created_ms=created_ms-604800001; UPDATE logout_intents SET created_ms=created_ms-604800001;").execute(&db.pool).await.unwrap();
    assert!(db.store.claim().await.unwrap().is_none());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM logout_deliveries")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        2
    );
    assert!(db.store.claim().await.unwrap().is_none());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM logout_deliveries")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM logout_intents")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM logout_audit")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        audits
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM relying_party_sessions")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        102
    );
    db.store.close().await;
}

#[tokio::test]
async fn logout_suppressed_audit_or_intent_cannot_report_a_successful_transition() {
    let db = configured().await;
    sqlx::raw_sql("CREATE FUNCTION suppress_logout() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NULL; END $$; CREATE TRIGGER suppress_logout BEFORE INSERT ON logout_audit FOR EACH ROW EXECUTE FUNCTION suppress_logout();").execute(&db.pool).await.unwrap();
    assert!(
        db.store
            .configure_logout([1; 32], target(), config(1, None))
            .await
            .is_err()
    );
    assert_eq!(
        db.store
            .logout_configuration([1; 32], target())
            .await
            .unwrap()
            .configuration
            .revision,
        1
    );
    sqlx::raw_sql("DROP TRIGGER suppress_logout ON logout_audit; CREATE TRIGGER suppress_logout BEFORE INSERT ON logout_intents FOR EACH ROW EXECUTE FUNCTION suppress_logout();").execute(&db.pool).await.unwrap();
    assert!(db.store.logout([1; 32]).await.is_err());
    assert!(db.store.session([1; 32]).await.is_ok());
    sqlx::raw_sql("DROP TRIGGER suppress_logout ON logout_intents; CREATE TRIGGER suppress_logout BEFORE INSERT ON logout_deliveries FOR EACH ROW EXECUTE FUNCTION suppress_logout();").execute(&db.pool).await.unwrap();
    db.store.logout([1; 32]).await.unwrap();
    assert!(db.store.claim().await.is_err());
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT expanded FROM logout_intents")
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
    db.store.close().await;
}
