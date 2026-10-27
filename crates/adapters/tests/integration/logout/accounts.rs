use super::*;

#[tokio::test]
async fn account_logout_upgrade_preserves_existing_intents_and_registrations() {
    let db = super::super::oidc::fixture_at_version(42).await;
    db.store
        .configure_logout(
            [1; 32],
            target(),
            config(0, Some("https://client.example/logout")),
        )
        .await
        .unwrap();
    sqlx::query("INSERT INTO logout_intents(session_id,created_ms) SELECT public_id,created_ms FROM browser_sessions").execute(&db.pool).await.unwrap();
    let before: (i64, Uuid, i64) =
        sqlx::query_as("SELECT id,session_id,created_ms FROM logout_intents")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    db.store.migrate().await.unwrap();
    let after: (i64, Uuid, i64) = sqlx::query_as(
        "SELECT id,session_id,created_ms FROM logout_intents WHERE destination_order IS NULL",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(before, after);
    let configuration = db
        .store
        .logout_configuration([1; 32], target())
        .await
        .unwrap()
        .configuration;
    assert_eq!(configuration.revision, 1);
    assert_eq!(
        configuration.uri.as_deref(),
        Some("https://client.example/logout")
    );
    assert_eq!(
        scalar(&db, "SELECT count(*) FROM logout_account_intents").await,
        0
    );
    assert!(db.store.session([1; 32]).await.is_ok());
    db.store
        .change(id(1), 0, AccountAction::RevokeAll)
        .await
        .unwrap();
    assert_eq!(
        scalar(&db, "SELECT count(*) FROM logout_account_intents").await,
        1
    );
    db.store.close().await;
}

#[tokio::test]
async fn account_logout_expired_pending_work_cannot_restart_the_delivery_window() {
    let db = configured().await;
    db.store
        .change(id(1), 0, AccountAction::RevokeAll)
        .await
        .unwrap();
    sqlx::query("UPDATE logout_account_intents SET created_ms=created_ms-86400001")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(db.store.claim().await.unwrap().is_none());
    assert_eq!(state(&db).await, "failed");
    assert!(db.store.session([1; 32]).await.is_err());
    assert_eq!(
        scalar(&db, "SELECT attempts::bigint FROM logout_deliveries").await,
        0
    );
    db.store.close().await;
}

#[tokio::test]
async fn account_revoke_all_notifies_old_sessions_and_preserves_new_logins() {
    let db = configured().await;
    let old_sid: Uuid = sqlx::query_scalar("SELECT sid FROM relying_party_sessions")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    db.store
        .change(id(1), 0, AccountAction::RevokeAll)
        .await
        .unwrap();
    assert!(db.store.session([1; 32]).await.is_err());
    let candidate = db
        .store
        .candidate("one@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [2; 32], None).await.unwrap();
    let delivery = db
        .store
        .claim()
        .await
        .unwrap()
        .expect("account invalidation must notify its relying parties");
    assert_eq!(delivery.claims.session.as_u128(), old_sid.as_u128());
    db.store
        .finish(delivery.id, delivery.attempt, DeliveryResult::Accepted)
        .await
        .unwrap();
    assert!(db.store.claim().await.unwrap().is_none());
    assert!(db.store.session([2; 32]).await.is_ok());
    db.store.close().await;
}

async fn scalar(db: &Database, query: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(query))
        .fetch_one(&db.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn account_logout_root_failure_or_suppression_rolls_back_authority_and_audit() {
    let db = configured().await;
    let audits = count(&db, "security_audit").await;
    for table in ["logout_account_intents", "security_audit"] {
        for body in ["RAISE EXCEPTION 'injected';", "RETURN NULL;"] {
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION reject_account_logout() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN {body} END $$; CREATE TRIGGER reject_account_logout BEFORE INSERT ON {table} FOR EACH ROW EXECUTE FUNCTION reject_account_logout();"))).execute(&db.pool).await.unwrap();
            assert!(
                db.store
                    .change(id(1), 0, AccountAction::RevokeAll)
                    .await
                    .is_err()
            );
            assert_eq!(db.store.account(id(1)).await.unwrap().credential_epoch, 0);
            assert_eq!(count(&db, "security_audit").await, audits);
            assert_eq!(
                scalar(&db, "SELECT count(*) FROM logout_account_intents").await,
                0
            );
            assert!(db.store.session([1; 32]).await.is_ok());
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP TRIGGER reject_account_logout ON {table}; DROP FUNCTION reject_account_logout();"))).execute(&db.pool).await.unwrap();
        }
    }
    db.store.close().await;
}

#[tokio::test]
async fn account_logout_overlap_keeps_original_registration_and_deadline() {
    for expanded in [false, true] {
        let db = configured().await;
        db.store
            .change(id(1), 0, AccountAction::RevokeAll)
            .await
            .unwrap();
        if expanded {
            let delivery = db.store.claim().await.unwrap().unwrap();
            db.store
                .finish(delivery.id, delivery.attempt, DeliveryResult::Accepted)
                .await
                .unwrap();
        }
        let candidate = db
            .store
            .candidate("one@example.com")
            .await
            .unwrap()
            .unwrap();
        db.store.establish(&candidate, [2; 32], None).await.unwrap();
        db.store
            .configure_logout(
                [2; 32],
                target(),
                config(1, Some("https://new.example/logout")),
            )
            .await
            .unwrap();
        db.store.logout([1; 32]).await.unwrap();
        assert!(db.store.claim().await.unwrap().is_none());
        assert_eq!(scalar(&db, "SELECT count(*) FROM logout_intents").await, 1);
        assert_eq!(
            scalar(
                &db,
                "SELECT count(*) FROM logout_deliveries WHERE revision=1"
            )
            .await,
            1
        );
        assert_eq!(scalar(&db, "SELECT count(*) FROM logout_intents i JOIN logout_account_intents a ON i.destination_order=a.id AND i.created_ms=a.created_ms").await, 1);
        assert_eq!(
            state(&db).await,
            if expanded { "delivered" } else { "cancelled" }
        );
        assert!(db.store.session([2; 32]).await.is_ok());
        db.store.close().await;
    }
}

#[tokio::test]
async fn account_logout_two_applications_multiple_sessions_excludes_other_accounts_and_epochs() {
    let db = configured().await;
    insert_principal(&db, 2, false).await;
    insert_password(&db, 2).await;
    for (email, handle) in [
        ("one@example.com", [2; 32]),
        ("person2@example.com", [3; 32]),
    ] {
        let candidate = db.store.candidate(email).await.unwrap().unwrap();
        db.store.establish(&candidate, handle, None).await.unwrap();
    }
    sqlx::raw_sql("INSERT INTO applications(id,name,owner_id,active) VALUES('00000000-0000-0000-0000-000000000011','Second app','00000000-0000-0000-0000-000000000001',true); INSERT INTO oauth_clients(id,application_id,name,active) VALUES('00000000-0000-0000-0000-000000000021','00000000-0000-0000-0000-000000000011','Second client',true);").execute(&db.pool).await.unwrap();
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
    sqlx::query("INSERT INTO relying_party_sessions(issuer,session_id,principal_id,client_id,created_ms) SELECT $1,s.public_id,s.principal_id,c.id,s.created_ms FROM browser_sessions s CROSS JOIN oauth_clients c ON CONFLICT(issuer,session_id,client_id) DO NOTHING").bind(ISSUER).execute(&db.pool).await.unwrap();
    db.store
        .change(id(1), 0, AccountAction::RevokeAll)
        .await
        .unwrap();
    let candidate = db
        .store
        .candidate("one@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [4; 32], None).await.unwrap();
    sqlx::query("INSERT INTO relying_party_sessions(issuer,session_id,principal_id,client_id,created_ms) SELECT $1,s.public_id,s.principal_id,c.id,s.created_ms FROM browser_sessions s CROSS JOIN oauth_clients c ON CONFLICT(issuer,session_id,client_id) DO NOTHING").bind(ISSUER).execute(&db.pool).await.unwrap();
    let expected: Vec<Uuid> = sqlx::query_scalar("SELECT r.sid FROM relying_party_sessions r JOIN browser_sessions s ON s.public_id=r.session_id WHERE s.principal_id=$1 AND s.credential_epoch=0 ORDER BY r.sid").bind(Uuid::from_u128(1)).fetch_all(&db.pool).await.unwrap();
    let mut actual = Vec::new();
    for _ in 0..4 {
        let delivery = db.store.claim().await.unwrap().unwrap();
        actual.push(Uuid::from_u128(delivery.claims.session.as_u128()));
        assert_eq!(delivery.claims.subject, id(1));
        db.store
            .finish(delivery.id, delivery.attempt, DeliveryResult::Accepted)
            .await
            .unwrap();
    }
    actual.sort();
    assert_eq!(actual, expected);
    assert!(db.store.claim().await.unwrap().is_none());
    for handle in [[3; 32], [4; 32]] {
        assert!(db.store.session(handle).await.is_ok());
    }
    db.store.close().await;
}

#[tokio::test]
async fn account_logout_expansion_is_paged_atomic_and_recoverable_from_an_independent_pool() {
    let db = configured().await;
    sqlx::raw_sql("INSERT INTO browser_sessions(digest,public_id,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms) SELECT decode(lpad(to_hex(n),64,'0'),'hex'),lpad(to_hex(n),32,'0')::uuid,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms FROM browser_sessions CROSS JOIN generate_series(1000,1204) n; INSERT INTO relying_party_sessions(issuer,session_id,principal_id,client_id,created_ms) SELECT r.issuer,s.public_id,s.principal_id,r.client_id,s.created_ms FROM relying_party_sessions r CROSS JOIN browser_sessions s ON CONFLICT(issuer,session_id,client_id) DO NOTHING;").execute(&db.pool).await.unwrap();
    db.store
        .change(id(1), 0, AccountAction::RevokeAll)
        .await
        .unwrap();
    assert_eq!(
        scalar(&db, "SELECT count(*) FROM logout_account_intents").await,
        1
    );
    assert_eq!(scalar(&db, "SELECT count(*) FROM logout_intents").await, 0);
    sqlx::raw_sql("CREATE FUNCTION suppress_account_expansion() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NULL; END $$; CREATE TRIGGER suppress_account_expansion BEFORE INSERT ON logout_intents FOR EACH ROW EXECUTE FUNCTION suppress_account_expansion();").execute(&db.pool).await.unwrap();
    assert!(db.store.claim().await.is_err());
    assert_eq!(
        scalar(
            &db,
            "SELECT count(*) FROM logout_account_intents WHERE cursor IS NULL AND NOT expanded"
        )
        .await,
        1
    );
    assert!(db.store.session([1; 32]).await.is_err());
    sqlx::query("DROP TRIGGER suppress_account_expansion ON logout_intents")
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
    // A skipped root can require a later pass; no cursor or session may be lost.
    other.claim().await.unwrap();
    assert_eq!(
        scalar(&db, "SELECT count(*) FROM logout_intents").await,
        206
    );
    assert_eq!(
        scalar(
            &db,
            "SELECT count(*) FROM logout_account_intents WHERE expanded"
        )
        .await,
        1
    );
    sqlx::query("UPDATE logout_account_intents SET created_ms=created_ms-604800001")
        .execute(&db.pool)
        .await
        .unwrap();
    other.claim().await.unwrap();
    assert_eq!(
        scalar(&db, "SELECT count(*) FROM logout_account_intents").await,
        0
    );
    // Child snapshots survive root retention.
    assert_eq!(
        scalar(
            &db,
            "SELECT count(*) FROM logout_intents WHERE destination_order IS NOT NULL"
        )
        .await,
        206
    );
    other.close().await;
    db.store.close().await;
}
