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
