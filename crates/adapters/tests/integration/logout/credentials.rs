use super::*;

async fn revoke(db: &Database) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE credentials SET revoked=true WHERE id=$1")
        .bind(Uuid::from_u128(101))
        .execute(&db.pool)
        .await?;
    Ok(())
}
async fn scalar(db: &Database, query: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(query))
        .fetch_one(&db.pool)
        .await
        .unwrap()
}
async fn prepared() -> Database {
    let db = configured().await;
    insert_principal(&db, 2, true).await;
    db
}

async fn replace_password(db: &Database) {
    sqlx::raw_sql("INSERT INTO credentials(id,principal_id,kind) VALUES('00000000-0000-0000-0000-000000000200','00000000-0000-0000-0000-000000000001','password'); INSERT INTO password_credentials(credential_id,verifier) SELECT '00000000-0000-0000-0000-000000000200',verifier FROM password_credentials WHERE credential_id='00000000-0000-0000-0000-000000000065';").execute(&db.pool).await.unwrap();
    let candidate = db
        .store
        .candidate("one@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [2; 32], None).await.unwrap();
}

async fn bind_sessions(db: &Database) {
    sqlx::query("INSERT INTO relying_party_sessions(issuer,session_id,principal_id,client_id,created_ms) SELECT $1,s.public_id,s.principal_id,c.id,s.created_ms FROM browser_sessions s CROSS JOIN oauth_clients c ON CONFLICT(issuer,session_id,client_id) DO NOTHING")
        .bind(ISSUER).execute(&db.pool).await.unwrap();
}

#[tokio::test]
async fn password_revocation_notifies_only_its_credential_and_epoch_without_reversing_local_denial()
{
    let db = prepared().await;
    super::receiver::second_application(&db).await;
    db.store
        .configure_logout(
            [1; 32],
            Target {
                application: ApplicationId::from_u128(17).unwrap(),
                client: ClientId::from_u128(33).unwrap(),
            },
            config(0, Some("https://second.example/logout")),
        )
        .await
        .unwrap();
    bind_sessions(&db).await;
    let expected: Vec<Uuid> =
        sqlx::query_scalar("SELECT sid FROM relying_party_sessions ORDER BY sid")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    revoke(&db).await.unwrap();
    replace_password(&db).await;
    let other = db
        .store
        .candidate("person2@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&other, [3; 32], None).await.unwrap();
    bind_sessions(&db).await;
    assert!(db.store.session([1; 32]).await.is_err());
    assert!(db.store.session([2; 32]).await.is_ok());
    assert!(db.store.session([3; 32]).await.is_ok());
    let mut actual = Vec::new();
    for _ in 0..2 {
        let delivery = db
            .store
            .claim()
            .await
            .unwrap()
            .expect("password revocation must notify the affected relying-party session");
        actual.push(Uuid::from_u128(delivery.claims.session.as_u128()));
        db.store
            .finish(delivery.id, delivery.attempt, DeliveryResult::Retry)
            .await
            .unwrap();
    }
    actual.sort();
    assert_eq!(actual, expected);
    assert!(db.store.session([1; 32]).await.is_err());
    revoke(&db).await.unwrap();
    assert_eq!(
        scalar(&db, "SELECT count(*) FROM credential_logout_events").await,
        1
    );
    assert!(db.store.claim().await.unwrap().is_none());
    db.store.close().await;
}

#[tokio::test]
async fn credential_intent_and_immutable_audit_failures_roll_back_revocation() {
    let db = prepared().await;
    for table in ["credential_logout_events", "logout_credential_intents"] {
        for body in ["RETURN NULL;", "RAISE EXCEPTION 'injected';"] {
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION fail_credential_logout() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN {body} END $$; CREATE TRIGGER fail_credential_logout BEFORE INSERT ON {table} FOR EACH ROW EXECUTE FUNCTION fail_credential_logout();"))).execute(&db.pool).await.unwrap();
            assert!(revoke(&db).await.is_err());
            assert!(db.store.session([1; 32]).await.is_ok());
            for relation in ["credential_logout_events", "logout_credential_intents"] {
                assert_eq!(
                    scalar(&db, &format!("SELECT count(*) FROM {relation}")).await,
                    0
                );
            }
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP TRIGGER fail_credential_logout ON {table}; DROP FUNCTION fail_credential_logout();"))).execute(&db.pool).await.unwrap();
        }
    }
    revoke(&db).await.unwrap();
    for statement in [
        "DELETE FROM credential_logout_events",
        "UPDATE credential_logout_events SET created_ms=0",
    ] {
        assert!(sqlx::query(statement).execute(&db.pool).await.is_err());
    }
    db.store.close().await;
}

#[tokio::test]
async fn last_administrator_rollback_noops_and_personal_keys_create_no_password_notifications() {
    let db = configured().await;
    assert!(revoke(&db).await.is_err());
    assert!(db.store.session([1; 32]).await.is_ok());
    sqlx::raw_sql("UPDATE credentials SET revoked=false; INSERT INTO credentials(id,principal_id,kind) VALUES('00000000-0000-0000-0000-000000000999','00000000-0000-0000-0000-000000000001','personal_key'); UPDATE credentials SET revoked=true WHERE kind='personal_key';").execute(&db.pool).await.unwrap();
    assert_eq!(
        scalar(&db, "SELECT count(*) FROM credential_logout_events").await,
        0
    );
    assert!(db.store.claim().await.unwrap().is_none());
    db.store.close().await;
}

#[tokio::test]
async fn credential_logout_upgrade_preserves_state_and_does_not_backfill_past_revocations() {
    let db = crate::oidc::fixture_at_version(43).await;
    insert_principal(&db, 2, true).await;
    db.store
        .configure_logout(
            [1; 32],
            target(),
            config(0, Some("https://client.example/logout")),
        )
        .await
        .unwrap();
    revoke(&db).await.unwrap();
    let original: Uuid = sqlx::query_scalar("SELECT public_id FROM browser_sessions")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    db.store.migrate().await.unwrap();
    db.store.migrate().await.unwrap();
    assert_eq!(
        scalar(&db, "SELECT count(*) FROM credential_logout_events").await,
        0
    );
    assert_eq!(
        scalar(&db, "SELECT count(*) FROM logout_destinations").await,
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>("SELECT public_id FROM browser_sessions")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        original
    );
    assert!(db.store.session([1; 32]).await.is_err());
    replace_password(&db).await;
    assert!(db.store.session([2; 32]).await.is_ok());
    db.store.close().await;
}

#[tokio::test]
async fn credential_notifications_exclude_sessions_from_previously_invalidated_epochs() {
    let db = prepared().await;
    db.store
        .change(id(1), 0, AccountAction::RevokeAll)
        .await
        .unwrap();
    let first = db.store.claim().await.unwrap().unwrap();
    let old = first.claims.session;
    db.store
        .finish(first.id, first.attempt, DeliveryResult::Accepted)
        .await
        .unwrap();
    let candidate = db
        .store
        .candidate("one@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [2; 32], None).await.unwrap();
    bind_sessions(&db).await;
    revoke(&db).await.unwrap();
    assert_eq!(
        scalar(&db, "SELECT credential_epoch FROM credential_logout_events").await,
        1
    );
    let current = db.store.claim().await.unwrap().unwrap();
    assert_ne!(current.claims.session, old);
    db.store
        .finish(current.id, current.attempt, DeliveryResult::Accepted)
        .await
        .unwrap();
    assert!(db.store.claim().await.unwrap().is_none());
    db.store.close().await;
}

#[tokio::test]
async fn account_expansion_and_individual_logout_keep_an_earlier_credential_destination_snapshot() {
    for account in [false, true] {
        let db = prepared().await;
        revoke(&db).await.unwrap();
        replace_password(&db).await;
        db.store
            .configure_logout(
                [2; 32],
                target(),
                config(1, Some("https://new.example/logout")),
            )
            .await
            .unwrap();
        if account {
            db.store
                .change(id(1), 0, AccountAction::RevokeAll)
                .await
                .unwrap();
        } else {
            db.store.logout([1; 32]).await.unwrap();
        }
        assert!(db.store.claim().await.unwrap().is_none());
        assert_eq!(state(&db).await, "cancelled");
        assert_eq!(scalar(&db,"SELECT count(*) FROM logout_intents i JOIN credential_logout_events e ON i.destination_order=e.ordering AND i.created_ms=e.created_ms").await,1);
        db.store.close().await;
    }
}

#[tokio::test]
async fn credential_revocation_takes_the_authority_fence_before_credential_row_locks() {
    let db = prepared().await;
    let mut reader = db.pool.begin().await.unwrap();
    sqlx::query("SELECT singleton FROM security_state FOR SHARE")
        .execute(&mut *reader)
        .await
        .unwrap();
    let revocation = revoke(&db);
    tokio::pin!(revocation);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut revocation)
            .await
            .is_err()
    );
    let live = sqlx::query_scalar::<_,bool>("SELECT NOT revoked FROM credentials WHERE id='00000000-0000-0000-0000-000000000065' FOR SHARE").fetch_one(&mut *reader);
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(2), live)
            .await
            .unwrap()
            .unwrap()
    );
    reader.commit().await.unwrap();
    revocation.await.unwrap();
    assert!(db.store.session([1; 32]).await.is_err());
    assert!(db.store.claim().await.unwrap().is_some());
    db.store.close().await;
}

#[tokio::test]
async fn credential_expansion_is_bounded_atomic_and_shared_across_worker_pools() {
    let db = prepared().await;
    sqlx::raw_sql("INSERT INTO browser_sessions(digest,public_id,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms) SELECT decode(lpad(to_hex(n),64,'0'),'hex'),lpad(to_hex(n),32,'0')::uuid,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms FROM browser_sessions CROSS JOIN generate_series(1000,1204) n;").execute(&db.pool).await.unwrap();
    bind_sessions(&db).await;
    revoke(&db).await.unwrap();
    assert_eq!(scalar(&db, "SELECT count(*) FROM logout_intents").await, 0);
    sqlx::raw_sql("CREATE FUNCTION suppress_credential_expansion() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NULL; END $$; CREATE TRIGGER suppress_credential_expansion BEFORE INSERT ON logout_intents FOR EACH ROW EXECUTE FUNCTION suppress_credential_expansion();").execute(&db.pool).await.unwrap();
    assert!(db.store.claim().await.is_err());
    assert_eq!(
        scalar(
            &db,
            "SELECT count(*) FROM logout_credential_intents WHERE cursor IS NULL AND NOT expanded"
        )
        .await,
        1
    );
    sqlx::query("DROP TRIGGER suppress_credential_expansion ON logout_intents")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(db.store.claim().await.unwrap().is_some());
    assert_eq!(
        scalar(&db, "SELECT count(*) FROM logout_intents").await,
        100
    );
    let other = PostgresStore::from_pool(
        PgPool::connect_with((*db.pool.connect_options()).clone())
            .await
            .unwrap(),
    );
    let (a, b) = tokio::join!(db.store.claim(), other.claim());
    a.unwrap();
    b.unwrap();
    other.claim().await.unwrap();
    assert_eq!(
        scalar(&db, "SELECT count(*) FROM logout_intents").await,
        206
    );
    assert_eq!(
        scalar(
            &db,
            "SELECT count(*) FROM logout_credential_intents WHERE expanded"
        )
        .await,
        1
    );
    other.close().await;
    db.store.close().await;
}

// Aging only disposable fixture rows; production event history is immutable.
async fn age(db: &Database, milliseconds: i64) {
    let mut tx = db.pool.begin().await.unwrap();
    sqlx::raw_sql("ALTER TABLE credential_logout_events DISABLE TRIGGER ALL; ALTER TABLE logout_credential_intents DISABLE TRIGGER ALL;").execute(&mut *tx).await.unwrap();
    for table in ["credential_logout_events", "logout_credential_intents"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE {table} SET created_ms=created_ms-$1"
        )))
        .bind(milliseconds)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    sqlx::raw_sql("ALTER TABLE credential_logout_events ENABLE TRIGGER ALL; ALTER TABLE logout_credential_intents ENABLE TRIGGER ALL;").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn expired_credential_work_never_restarts_its_delivery_window_and_retains_event_history() {
    let db = prepared().await;
    revoke(&db).await.unwrap();
    age(&db, 86400001).await;
    assert!(db.store.claim().await.unwrap().is_none());
    assert_eq!(state(&db).await, "failed");
    assert_eq!(
        scalar(&db, "SELECT attempts::bigint FROM logout_deliveries").await,
        0
    );
    age(&db, 604800001).await;
    assert!(db.store.claim().await.unwrap().is_none());
    assert_eq!(
        scalar(&db, "SELECT count(*) FROM logout_credential_intents").await,
        0
    );
    assert_eq!(
        scalar(&db, "SELECT count(*) FROM credential_logout_events").await,
        1
    );
    // Simulate terminal payload retention; a later explicit logout must still use the old event.
    sqlx::raw_sql("DELETE FROM logout_deliveries; DELETE FROM logout_intents;")
        .execute(&db.pool)
        .await
        .unwrap();
    replace_password(&db).await;
    db.store
        .configure_logout(
            [2; 32],
            target(),
            config(1, Some("https://new.example/logout")),
        )
        .await
        .unwrap();
    db.store.logout([1; 32]).await.unwrap();
    assert_eq!(scalar(&db,"SELECT count(*) FROM logout_intents i JOIN credential_logout_events e ON i.destination_order=e.ordering AND i.created_ms=e.created_ms").await,1);
    assert!(db.store.claim().await.unwrap().is_none());
    db.store.close().await;
}
