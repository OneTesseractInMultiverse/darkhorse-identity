use super::*;

#[tokio::test]
async fn runtime_commands_disclose_no_record_or_target_error_after_audit_authority_loss() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    restrict_database(&f).await;
    // Another eligible administrator makes reductions valid independently of test order.
    actor(&f).await;
    for (table, cases) in [
        (
            "operator_account_audit",
            vec![
                vec!["account", "self"],
                vec!["account", "missing"],
                vec!["revoke-all", "self", "99"],
                vec!["revoke-all", "self", "0"],
            ],
        ),
        (
            "operator_directory_audit",
            vec![vec!["operator", "account", "list"]],
        ),
        (
            "operator_catalog_audit",
            vec![
                vec!["operator", "application", "list"],
                vec!["operator", "client", "list", "missing"],
            ],
        ),
    ] {
        let (id, email) = actor(&f).await;
        let target = id.to_string();
        let missing = Uuid::new_v4().to_string();
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION reduce_runtime_actor() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE credentials SET revoked=true WHERE id=NEW.actor_credential_id; RETURN NEW; END $$; CREATE TRIGGER reduce_runtime_actor AFTER INSERT ON {table} FOR EACH ROW EXECUTE FUNCTION reduce_runtime_actor();"))).execute(&f.pool).await.unwrap();
        for args in cases {
            let args = args
                .into_iter()
                .map(|arg| match arg {
                    "self" => target.as_str(),
                    "missing" => missing.as_str(),
                    _ => arg,
                })
                .collect::<Vec<_>>();
            let protected = if args.last() == Some(&"list") || args.contains(&"list") {
                json!({"email":email,"password":PASSWORD})
            } else {
                input(&email)
            };
            let (code, value) = invoke(&args, protected);
            assert_eq!(code, 1, "{value}");
            assert_eq!(
                value["error"]["message"],
                "Administrator authentication or authority denied."
            );
            for field in ["items", "record", "email", "revision", "active"] {
                assert!(value["data"].get(field).is_none());
            }
            assert_eq!(
                sqlx::query_as::<_, (i64, i64)>(
                    "SELECT revision,credential_epoch FROM principals WHERE id=$1"
                )
                .bind(id)
                .fetch_one(&f.pool)
                .await
                .unwrap(),
                (0, 0)
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
                    "SELECT count(*) FROM {table} WHERE actor_id=$1"
                )))
                .bind(id)
                .fetch_one(&f.pool)
                .await
                .unwrap(),
                0
            );
        }
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "DROP TRIGGER reduce_runtime_actor ON {table}; DROP FUNCTION reduce_runtime_actor();"
        )))
        .execute(&f.pool)
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn runtime_self_changes_commit_once_even_when_the_output_stream_is_lost() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    restrict_database(&f).await;
    actor(&f).await;
    let (id, email) = actor(&f).await;
    let target = id.to_string();
    let revoked = succeeds(&["operator", "account", "revoke-all", &target, "0"], &email);
    assert_eq!(revoked["revision"], 1);
    assert_eq!(revoked["changed"], true);
    let (code, value) = invoke_output(
        "runtime",
        &["operator", "account", "deactivate", &target, "1"],
        input(&email),
        true,
    );
    assert_eq!(code, 74, "{value}");
    assert_eq!(
        sqlx::query_as::<_, (bool, i64, i64)>(
            "SELECT active,revision,credential_epoch FROM principals WHERE id=$1"
        )
        .bind(id)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        (false, 2, 2)
    );
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM operator_account_audit WHERE actor_id=$1 AND result='changed' AND database_role='darkhorse_runtime'").bind(id).fetch_one(&f.pool).await.unwrap(),2);
    let (code, value) = invoke(&["account", &target], input(&email));
    assert_eq!(code, 1);
    assert_eq!(
        value["error"]["message"],
        "Administrator authentication or authority denied."
    );
}

#[tokio::test]
async fn runtime_suppressed_account_effects_return_unavailable_and_leave_no_success_audit() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    restrict_database(&f).await;
    for table in ["principals", "security_audit"] {
        let (id, email) = actor(&f).await;
        let event = if table == "principals" {
            "UPDATE"
        } else {
            "INSERT"
        };
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION suppress_runtime_effect() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NULL; END $$; CREATE TRIGGER suppress_runtime_effect BEFORE {event} ON {table} FOR EACH ROW EXECUTE FUNCTION suppress_runtime_effect();"))).execute(&f.pool).await.unwrap();
        let (code, value) = invoke(&["revoke-all", &id.to_string(), "0"], input(&email));
        assert_eq!(code, 1);
        assert!(
            value["error"]["message"]
                .as_str()
                .unwrap()
                .contains("unavailable")
        );
        assert_eq!(
            sqlx::query_as::<_, (i64, i64)>(
                "SELECT revision,credential_epoch FROM principals WHERE id=$1"
            )
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
            (0, 0)
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM operator_account_audit WHERE actor_id=$1"
            )
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
            0
        );
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP TRIGGER suppress_runtime_effect ON {table}; DROP FUNCTION suppress_runtime_effect();"))).execute(&f.pool).await.unwrap();
    }
}
