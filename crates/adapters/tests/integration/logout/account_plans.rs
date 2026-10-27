use super::*;

#[tokio::test]
async fn account_logout_expansion_plans_seek_the_epoch_cursor_at_large_population() {
    let db = configured().await;
    sqlx::raw_sql("INSERT INTO browser_sessions(digest,public_id,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms) SELECT decode(lpad(to_hex(n),64,'0'),'hex'),lpad(to_hex(n),32,'0')::uuid,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms FROM browser_sessions CROSS JOIN generate_series(1000,10999) n; INSERT INTO relying_party_sessions(issuer,session_id,principal_id,client_id,created_ms) SELECT r.issuer,s.public_id,s.principal_id,r.client_id,s.created_ms FROM relying_party_sessions r CROSS JOIN browser_sessions s ON CONFLICT(issuer,session_id,client_id) DO NOTHING; ANALYZE browser_sessions; ANALYZE relying_party_sessions;").execute(&db.pool).await.unwrap();
    for principal in 2..=101 {
        insert_principal(&db, principal, false).await;
        insert_password(&db, principal).await;
    }
    sqlx::raw_sql("INSERT INTO browser_sessions(digest,public_id,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms) SELECT decode(lpad(to_hex(20000+row_number() OVER()),64,'0'),'hex'),lpad(to_hex(20000+row_number() OVER()),32,'0')::uuid,c.principal_id,c.id,0,0,0,28800000 FROM credentials c CROSS JOIN generate_series(1,100) n WHERE c.kind='password' AND c.principal_id<>'00000000-0000-0000-0000-000000000001'; ANALYZE browser_sessions;").execute(&db.pool).await.unwrap();
    db.store
        .change(id(1), 0, AccountAction::RevokeAll)
        .await
        .unwrap();
    let root: (i64, i64) = sqlx::query_as("SELECT id,created_ms FROM logout_account_intents")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let statement = format!(
        "PREPARE account_logout_expand(uuid,bigint,uuid,bigint,bigint) AS {}",
        include_str!("../../../src/postgres/logout/accounts-expand.sql")
    );
    for cursor in [None, Some(Uuid::from_u128(10500))] {
        let mut tx = db.pool.begin().await.unwrap();
        sqlx::query("SELECT singleton FROM security_state FOR SHARE")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("SET LOCAL plan_cache_mode=force_generic_plan")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement.as_str()))
            .execute(&mut *tx)
            .await
            .unwrap();
        let cursor_sql = cursor.map_or_else(|| "NULL".to_owned(), |id| format!("'{id}'"));
        let explain = format!(
            "EXPLAIN (ANALYZE, BUFFERS) EXECUTE account_logout_expand('00000000-0000-0000-0000-000000000001',0,{cursor_sql},{},{})",
            root.0, root.1
        );
        let lines: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(explain))
            .fetch_all(&mut *tx)
            .await
            .unwrap();
        sqlx::query("DEALLOCATE account_logout_expand")
            .execute(&mut *tx)
            .await
            .unwrap();
        let plan = lines.join("\n");
        assert!(plan.contains("browser_sessions_logout_epoch"), "{plan}");
        assert!(plan.contains("public_id >"), "{plan}");
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM logout_intents")
                .fetch_one(&mut *tx)
                .await
                .unwrap(),
            100
        );
        println!(
            "Account logout expansion, 10001 target sessions and 10000 sessions across 100 other accounts, generic plan, cursor {cursor:?}:\n{plan}"
        );
        tx.rollback().await.unwrap();
    }
    db.store.close().await;
}
