use super::*;
use darkhorse_application::{
    oidc_maintenance::{CredentialRecordCategory, CredentialRecordMaintenance},
    signing::SigningStore,
};

const ISSUER: &str = "https://issuer.example";
const PRINCIPAL_ID: Uuid = Uuid::from_u128(1);
const CLIENT_ID: Uuid = Uuid::from_u128(32);

async fn historical_session(db: &Database, marker: u8, age_ms: i64) -> ([u8; 32], i64) {
    let (credential_id, epoch): (Uuid, i64) = sqlx::query_as(
        "SELECT c.id,p.credential_epoch FROM principals p JOIN credentials c ON c.principal_id=p.id JOIN password_credentials pc ON pc.credential_id=c.id WHERE p.id=$1",
    )
    .bind(PRINCIPAL_ID)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let (created_ms,): (i64,) =
        sqlx::query_as("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint-$1")
            .bind(age_ms)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let digest = [marker; 32];
    sqlx::query(
        "INSERT INTO browser_sessions(digest,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms) VALUES($1,$2,$3,$4,$5,$5,$5+28800000)",
    )
    .bind(digest.as_slice())
    .bind(PRINCIPAL_ID)
    .bind(credential_id)
    .bind(epoch)
    .bind(created_ms)
    .execute(&db.pool)
    .await
    .unwrap();
    (digest, created_ms)
}

async fn seed_expired_legacy_credentials(
    db: &Database,
    session_digest: [u8; 32],
    authenticated_ms: i64,
    count: i64,
) {
    sqlx::query(
        "INSERT INTO authorization_codes(digest,request_digest,client_id,client_revision,application_revision,session_digest,principal_id,authenticated_ms,redirect_uri,challenge,created_ms,expires_ms,consumed,scopes,capability_ceiling) SELECT decode(lpad(to_hex(n+100000),64,'0'),'hex'),decode(lpad(to_hex(n+200000),64,'0'),'hex'),$1,0,0,$2,$3,$4,'https://client.example/callback?fixed=1',$5,$4+30000,$4+90000,true,ARRAY['openid'],ARRAY[]::uuid[] FROM generate_series(1,$6) AS rows(n)",
    )
    .bind(CLIENT_ID)
    .bind(session_digest.as_slice())
    .bind(PRINCIPAL_ID)
    .bind(authenticated_ms)
    .bind([7u8; 32].as_slice())
    .bind(count)
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO access_tokens(digest,code_digest,audience,scope,claim_ceiling,capability_ceiling,created_ms,expires_ms,resource_id,refresh_generation) SELECT decode(lpad(to_hex(n+300000),64,'0'),'hex'),decode(lpad(to_hex(n+100000),64,'0'),'hex'),$1,'openid',ARRAY['sub'],ARRAY[]::uuid[],$2+90001,$2+390001,NULL,NULL FROM generate_series(1,$3) AS rows(n)",
    )
    .bind(format!("{ISSUER}/userinfo"))
    .bind(authenticated_ms)
    .bind(count)
    .execute(&db.pool)
    .await
    .unwrap();
}

async fn seed_orphan_expired_code(db: &Database, digest_marker: u8) -> [u8; 32] {
    let (session_digest, authenticated_ms) = historical_session(db, digest_marker, 900_000).await;
    let digest = [digest_marker.wrapping_add(1); 32];
    sqlx::query(
        "INSERT INTO authorization_codes(digest,request_digest,client_id,client_revision,application_revision,session_digest,principal_id,authenticated_ms,redirect_uri,challenge,created_ms,expires_ms,consumed,scopes,capability_ceiling) VALUES($1,$2,$3,0,0,$4,$5,$6,'https://client.example/callback?fixed=1',$7,$6+30000,$6+90000,false,ARRAY['openid'],ARRAY[]::uuid[])",
    )
    .bind(digest.as_slice())
    .bind([digest_marker.wrapping_add(2); 32].as_slice())
    .bind(CLIENT_ID)
    .bind(session_digest.as_slice())
    .bind(PRINCIPAL_ID)
    .bind(authenticated_ms)
    .bind([8u8; 32].as_slice())
    .execute(&db.pool)
    .await
    .unwrap();
    digest
}

async fn seed_live_credentials(db: &Database) -> ([u8; 32], [u8; 32]) {
    let (session_digest, authenticated_ms) = historical_session(db, 0x41, 0).await;
    let code_digest: [u8; 32] = [0x42; 32];
    let token_digest: [u8; 32] = [0x43; 32];
    sqlx::query(
        "INSERT INTO authorization_codes(digest,request_digest,client_id,client_revision,application_revision,session_digest,principal_id,authenticated_ms,redirect_uri,challenge,created_ms,expires_ms,consumed,scopes,capability_ceiling) VALUES($1,$2,$3,0,0,$4,$5,$6,'https://client.example/callback?fixed=1',$7,$6+30000,$6+90000,true,ARRAY['openid'],ARRAY[]::uuid[])",
    )
    .bind(code_digest.as_slice())
    .bind([0x44u8; 32].as_slice())
    .bind(CLIENT_ID)
    .bind(session_digest.as_slice())
    .bind(PRINCIPAL_ID)
    .bind(authenticated_ms)
    .bind([9u8; 32].as_slice())
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO access_tokens(digest,code_digest,audience,scope,claim_ceiling,capability_ceiling,created_ms,expires_ms,resource_id,refresh_generation) VALUES($1,$2,$3,'openid',ARRAY['sub'],ARRAY[]::uuid[],$4+90001,$4+390001,NULL,NULL)",
    )
    .bind(token_digest.as_slice())
    .bind(code_digest.as_slice())
    .bind(format!("{ISSUER}/userinfo"))
    .bind(authenticated_ms)
    .execute(&db.pool)
    .await
    .unwrap();
    (code_digest, token_digest)
}

async fn seed_refresh_family_credentials(db: &Database) -> ([u8; 32], [u8; 32]) {
    let (session_digest, authenticated_ms) = historical_session(db, 0x51, 900_000).await;
    let code_digest: [u8; 32] = [0x52; 32];
    let access_digest: [u8; 32] = [0x53; 32];
    let refresh_digest: [u8; 32] = [0x54; 32];
    sqlx::query(
        "INSERT INTO authorization_codes(digest,request_digest,client_id,client_revision,application_revision,session_digest,principal_id,authenticated_ms,redirect_uri,challenge,created_ms,expires_ms,consumed,scopes,capability_ceiling) VALUES($1,$2,$3,0,0,$4,$5,$6,'https://client.example/callback?fixed=1',$7,$6+30000,$6+90000,true,ARRAY['openid'],ARRAY[]::uuid[])",
    )
    .bind(code_digest.as_slice())
    .bind([0x55u8; 32].as_slice())
    .bind(CLIENT_ID)
    .bind(session_digest.as_slice())
    .bind(PRINCIPAL_ID)
    .bind(authenticated_ms)
    .bind([10u8; 32].as_slice())
    .execute(&db.pool)
    .await
    .unwrap();
    let created_ms = authenticated_ms + 90_001;
    sqlx::query(
        "INSERT INTO refresh_families(code_digest,issuer,created_ms,expires_ms) VALUES($1,$2,$3,$4)",
    )
    .bind(code_digest.as_slice())
    .bind(ISSUER)
    .bind(created_ms)
    .bind(authenticated_ms + 28_800_000)
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO refresh_tokens(digest,code_digest,generation,scope,capability_ceiling,created_ms,expires_ms) VALUES($1,$2,0,'openid',ARRAY[]::uuid[],$3,$4)",
    )
    .bind(refresh_digest.as_slice())
    .bind(code_digest.as_slice())
    .bind(created_ms)
    .bind(created_ms + 600_000)
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO access_tokens(digest,code_digest,audience,scope,claim_ceiling,capability_ceiling,created_ms,expires_ms,resource_id,refresh_generation) VALUES($1,$2,$3,'openid',ARRAY['sub'],ARRAY[]::uuid[],$4,$5,NULL,0)",
    )
    .bind(access_digest.as_slice())
    .bind(code_digest.as_slice())
    .bind(format!("{ISSUER}/userinfo"))
    .bind(created_ms)
    .bind(created_ms + 300_000)
    .execute(&db.pool)
    .await
    .unwrap();
    (code_digest, access_digest)
}

#[tokio::test]
async fn cleanup_bounds_batches_and_retains_live_or_family_bound_credentials() {
    let db = fixture().await;
    db.store.bind_provider(ISSUER, [1; 32]).await.unwrap();
    sqlx::query("UPDATE oauth_clients SET refresh_tokens=true,revision=revision+1 WHERE id=$1")
        .bind(CLIENT_ID)
        .execute(&db.pool)
        .await
        .unwrap();
    let (expired_session, expired_authenticated_ms) = historical_session(&db, 0x31, 900_000).await;
    seed_expired_legacy_credentials(&db, expired_session, expired_authenticated_ms, 105).await;
    let expired_orphan = seed_orphan_expired_code(&db, 0x61).await;
    let (live_code, live_access) = seed_live_credentials(&db).await;
    let (family_code, family_access) = seed_refresh_family_credentials(&db).await;

    let first = db
        .store
        .prune_expired_credential_records(CredentialRecordCategory::LegacyAccessTokens)
        .await
        .unwrap();
    assert_eq!(first.deleted, 100);
    assert!(first.backlog_remaining);
    assert!(first.oldest_expired_age_ms.is_some());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM access_tokens WHERE refresh_generation IS NULL AND expires_ms<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        5
    );

    let first_code_cleanup = db
        .store
        .prune_expired_credential_records(CredentialRecordCategory::AuthorizationCodes)
        .await
        .unwrap();
    assert_eq!(first_code_cleanup.deleted, 100);
    assert!(first_code_cleanup.backlog_remaining);
    let orphan_cleanup = db
        .store
        .prune_expired_credential_records(CredentialRecordCategory::AuthorizationCodes)
        .await
        .unwrap();
    assert_eq!(orphan_cleanup.deleted, 1);
    assert!(!orphan_cleanup.backlog_remaining);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM authorization_codes WHERE digest=$1")
            .bind(expired_orphan.as_slice())
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );

    let remainder = db
        .store
        .prune_expired_credential_records(CredentialRecordCategory::LegacyAccessTokens)
        .await
        .unwrap();
    assert_eq!(remainder.deleted, 5);
    assert!(!remainder.backlog_remaining);
    let expired_codes = db
        .store
        .prune_expired_credential_records(CredentialRecordCategory::AuthorizationCodes)
        .await
        .unwrap();
    assert_eq!(expired_codes.deleted, 5);
    assert!(!expired_codes.backlog_remaining);

    for (table, digest) in [
        ("authorization_codes", live_code),
        ("authorization_codes", family_code),
        ("access_tokens", live_access),
        ("access_tokens", family_access),
    ] {
        let query = match table {
            "authorization_codes" => "SELECT count(*) FROM authorization_codes WHERE digest=$1",
            "access_tokens" => "SELECT count(*) FROM access_tokens WHERE digest=$1",
            _ => unreachable!(),
        };
        assert_eq!(
            sqlx::query_scalar::<_, i64>(query)
                .bind(digest.as_slice())
                .fetch_one(&db.pool)
                .await
                .unwrap(),
            1,
            "retained {table} row"
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM refresh_families WHERE code_digest=$1")
            .bind(family_code.as_slice())
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    db.store.close().await;
}

#[tokio::test]
async fn credential_cleanup_workers_skip_busy_rows_and_delete_disjoint_bounded_batches() {
    let db = fixture().await;
    db.store.bind_provider(ISSUER, [1; 32]).await.unwrap();
    let (session_digest, authenticated_ms) = historical_session(&db, 0x71, 900_000).await;
    seed_expired_legacy_credentials(&db, session_digest, authenticated_ms, 205).await;

    let mut token_blocker = db.pool.begin().await.unwrap();
    let locked_token: Vec<u8> = sqlx::query_scalar(
        "SELECT digest FROM access_tokens WHERE refresh_generation IS NULL ORDER BY expires_ms,digest LIMIT 1 FOR UPDATE",
    )
    .fetch_one(&mut *token_blocker)
    .await
    .unwrap();
    let token_batch = db
        .store
        .prune_expired_credential_records(CredentialRecordCategory::LegacyAccessTokens)
        .await
        .unwrap();
    assert_eq!(token_batch.deleted, 100);
    assert!(token_batch.backlog_remaining);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM access_tokens WHERE digest=$1")
            .bind(&locked_token)
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1,
        "a locked token must remain available for a later sweep"
    );
    token_blocker.rollback().await.unwrap();

    let first_store = db.store.clone();
    let second_store = db.store.clone();
    let (first_token_batch, second_token_batch) = tokio::join!(
        first_store.prune_expired_credential_records(CredentialRecordCategory::LegacyAccessTokens),
        second_store.prune_expired_credential_records(CredentialRecordCategory::LegacyAccessTokens),
    );
    let first_token_batch = first_token_batch.unwrap();
    let second_token_batch = second_token_batch.unwrap();
    assert!(first_token_batch.deleted <= 100);
    assert!(second_token_batch.deleted <= 100);
    assert_eq!(first_token_batch.deleted + second_token_batch.deleted, 105);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM access_tokens")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0,
        "overlapping sweeps must not leave duplicate or skipped token work"
    );

    let mut code_blocker = db.pool.begin().await.unwrap();
    let locked_code: Vec<u8> = sqlx::query_scalar(
        "SELECT digest FROM authorization_codes ORDER BY expires_ms,digest LIMIT 1 FOR UPDATE",
    )
    .fetch_one(&mut *code_blocker)
    .await
    .unwrap();
    let code_batch = db
        .store
        .prune_expired_credential_records(CredentialRecordCategory::AuthorizationCodes)
        .await
        .unwrap();
    assert_eq!(code_batch.deleted, 100);
    assert!(code_batch.backlog_remaining);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM authorization_codes WHERE digest=$1")
            .bind(&locked_code)
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1,
        "a locked code must remain available for a later sweep"
    );
    code_blocker.rollback().await.unwrap();

    let first_store = db.store.clone();
    let second_store = db.store.clone();
    let (first_code_batch, second_code_batch) = tokio::join!(
        first_store.prune_expired_credential_records(CredentialRecordCategory::AuthorizationCodes),
        second_store.prune_expired_credential_records(CredentialRecordCategory::AuthorizationCodes),
    );
    let first_code_batch = first_code_batch.unwrap();
    let second_code_batch = second_code_batch.unwrap();
    assert!(first_code_batch.deleted <= 100);
    assert!(second_code_batch.deleted <= 100);
    assert_eq!(first_code_batch.deleted + second_code_batch.deleted, 105);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM authorization_codes")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0,
        "overlapping sweeps must not leave duplicate or skipped code work"
    );
    db.store.close().await;
}

#[tokio::test]
async fn failed_legacy_token_cleanup_rolls_back_and_can_be_retried() {
    let db = fixture().await;
    let (session_digest, authenticated_ms) = historical_session(&db, 0x81, 900_000).await;
    seed_expired_legacy_credentials(&db, session_digest, authenticated_ms, 3).await;
    sqlx::raw_sql("CREATE FUNCTION reject_legacy_token_delete() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected cleanup failure'; END $$; CREATE TRIGGER reject_legacy_token_delete BEFORE DELETE ON access_tokens FOR EACH ROW EXECUTE FUNCTION reject_legacy_token_delete();")
        .execute(&db.pool)
        .await
        .unwrap();

    assert_eq!(
        db.store
            .prune_expired_credential_records(CredentialRecordCategory::LegacyAccessTokens)
            .await,
        Err(Error::Unavailable)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM access_tokens")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        3,
        "a failed delete transaction must retain all rows"
    );

    sqlx::query("DROP TRIGGER reject_legacy_token_delete ON access_tokens")
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION reject_legacy_token_delete()")
        .execute(&db.pool)
        .await
        .unwrap();
    let retried = db
        .store
        .prune_expired_credential_records(CredentialRecordCategory::LegacyAccessTokens)
        .await
        .unwrap();
    assert_eq!(retried.deleted, 3);
    assert!(!retried.backlog_remaining);
    db.store.close().await;
}

#[tokio::test]
async fn interrupted_authorization_code_cleanup_rolls_back_and_can_be_retried() {
    let db = fixture().await;
    let code_digest = seed_orphan_expired_code(&db, 0x91).await;
    sqlx::raw_sql("CREATE FUNCTION block_expired_code_delete() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(314160,271829); RETURN OLD; END $$; CREATE TRIGGER block_expired_code_delete BEFORE DELETE ON authorization_codes FOR EACH ROW EXECUTE FUNCTION block_expired_code_delete();")
        .execute(&db.pool)
        .await
        .unwrap();

    let mut blocker = db.pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(314160,271829)")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let store = db.store.clone();
    let operation = tokio::spawn(async move {
        store
            .prune_expired_credential_records(CredentialRecordCategory::AuthorizationCodes)
            .await
    });
    let backend = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let pid = sqlx::query_scalar::<_, i32>(
                "SELECT pid FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND wait_event_type='Lock' AND query LIKE 'WITH expired AS MATERIALIZED%' ORDER BY query_start DESC LIMIT 1",
            )
            .fetch_optional(&db.pool)
            .await
            .unwrap();
            if let Some(pid) = pid {
                break pid;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("cleanup reached the deliberately blocked code delete");
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT pg_terminate_backend($1)")
            .bind(backend)
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
    assert_eq!(operation.await.unwrap(), Err(Error::Unavailable));
    blocker.rollback().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM authorization_codes WHERE digest=$1")
            .bind(code_digest.as_slice())
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1,
        "terminating the cleanup backend must roll back the code delete"
    );

    sqlx::query("DROP TRIGGER block_expired_code_delete ON authorization_codes")
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION block_expired_code_delete()")
        .execute(&db.pool)
        .await
        .unwrap();
    let retried = db
        .store
        .prune_expired_credential_records(CredentialRecordCategory::AuthorizationCodes)
        .await
        .unwrap();
    assert_eq!(retried.deleted, 1);
    assert!(!retried.backlog_remaining);
    db.store.close().await;
}
