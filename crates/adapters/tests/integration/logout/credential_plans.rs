use super::*;

#[tokio::test]
async fn credential_logout_generic_plan_seeks_credential_epoch_and_cursor_with_unrelated_population()
 {
    let db = configured().await;
    insert_principal(&db, 2, true).await;
    sqlx::raw_sql("INSERT INTO browser_sessions(digest,public_id,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms) SELECT decode(lpad(to_hex(n),64,'0'),'hex'),lpad(to_hex(n),32,'0')::uuid,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms FROM browser_sessions CROSS JOIN generate_series(1000,10999) n; INSERT INTO browser_sessions(digest,public_id,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms) SELECT decode(lpad(to_hex(n),64,'0'),'hex'),lpad(to_hex(n),32,'0')::uuid,'00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-000000000065',1,0,0,28800000 FROM generate_series(20000,29999) n; INSERT INTO browser_sessions(digest,public_id,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms) SELECT decode(lpad(to_hex(n),64,'0'),'hex'),lpad(to_hex(n),32,'0')::uuid,'00000000-0000-0000-0000-000000000002','00000000-0000-0000-0000-000000000066',0,0,0,28800000 FROM generate_series(30000,39999) n; INSERT INTO relying_party_sessions(issuer,session_id,principal_id,client_id,created_ms) SELECT r.issuer,s.public_id,s.principal_id,r.client_id,s.created_ms FROM relying_party_sessions r CROSS JOIN browser_sessions s ON CONFLICT(issuer,session_id,client_id) DO NOTHING; ANALYZE browser_sessions; ANALYZE relying_party_sessions;").execute(&db.pool).await.unwrap();
    sqlx::query(
        "UPDATE credentials SET revoked=true WHERE id='00000000-0000-0000-0000-000000000065'",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    let root: (i64, i64) =
        sqlx::query_as("SELECT ordering,created_ms FROM credential_logout_events")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let query = include_str!("../../../src/postgres/logout/credentials-expand.sql");
    // Same authority fence and writes; compare the original scalar cursor with
    // the bounded prefix range using one population and generic prepared plans.
    let baseline = format!(
        "WITH targets AS MATERIALIZED (SELECT public_id FROM browser_sessions WHERE principal_id=$1 AND credential_id=$2 AND credential_epoch=$3 AND public_id>COALESCE($4::uuid,'00000000-0000-0000-0000-000000000000'::uuid) ORDER BY public_id LIMIT 100), eligible AS MATERIALIZED ({}",
        query.split_once("), eligible AS MATERIALIZED (").unwrap().1
    );
    for (variant, query) in [
        ("scalar-cursor", baseline.as_str()),
        ("prefix-range", query),
    ] {
        let statement = format!(
            "PREPARE credential_logout_expand(uuid,uuid,bigint,uuid,bigint,bigint) AS {}",
            query
        );
        for cursor in [
            None,
            Some(Uuid::from_u128(10500)),
            Some(Uuid::from_u128(10995)),
        ] {
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
                "EXPLAIN (ANALYZE, BUFFERS) EXECUTE credential_logout_expand('00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-000000000065',0,{cursor_sql},{},{})",
                root.0, root.1
            );
            let lines: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(explain))
                .fetch_all(&mut *tx)
                .await
                .unwrap();
            let plan = lines.join("\n");
            if variant == "prefix-range" {
                assert!(
                    plan.contains("browser_sessions_logout_credential"),
                    "{plan}"
                );
                for condition in [
                    "ROW(credential_id, credential_epoch, principal_id, public_id)",
                    "public_id) >",
                    "public_id) <=",
                ] {
                    assert!(plan.contains(condition), "{plan}");
                }
                assert!(
                    !plan
                        .split("CTE eligible")
                        .next()
                        .unwrap()
                        .contains("Filter:"),
                    "{plan}"
                );
            }
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM logout_intents i JOIN browser_sessions s ON s.public_id=i.session_id WHERE s.credential_id<>'00000000-0000-0000-0000-000000000065' OR s.credential_epoch<>0 OR s.principal_id<>'00000000-0000-0000-0000-000000000001'").fetch_one(&mut *tx).await.unwrap(),0);
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM logout_intents")
                    .fetch_one(&mut *tx)
                    .await
                    .unwrap(),
                if cursor == Some(Uuid::from_u128(10995)) {
                    5
                } else {
                    100
                }
            );
            println!(
                "Credential logout expansion {variant}: 10001 target, 10000 other epoch, 10000 other principal/credential, generic plan, cursor {cursor:?}:\n{plan}"
            );
            sqlx::query("DEALLOCATE credential_logout_expand")
                .execute(&mut *tx)
                .await
                .unwrap();
            tx.rollback().await.unwrap();
        }
    }
    db.store.close().await;
}
