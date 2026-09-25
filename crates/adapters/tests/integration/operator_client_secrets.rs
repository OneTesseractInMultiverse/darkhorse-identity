use super::operator_directory::{Allow, Held, Password};
use super::*;
use darkhorse_application::{
    operator_accounts::{self, Store},
    operator_client_secrets::{Inventory, Outcome},
    registration::{ClientAuthenticationStore, Command, Prepared, RegistrationStore},
};
use darkhorse_domain::{
    identity::{ApplicationId, ClientId, ClientSecretId, OperationId},
    operator_accounts::Error,
    operator_client_secrets::{Operation, Request, Target},
    registration::RegistrationError,
};
fn target() -> Target {
    Target {
        application: ApplicationId::from_u128(16).unwrap(),
        client: ClientId::from_u128(32).unwrap(),
    }
}
fn list(after: Option<u128>, limit: u16) -> Request {
    Request::new(
        target(),
        Operation::List {
            after: after.map(|n| ClientSecretId::from_u128(n).unwrap()),
            limit,
        },
        None,
    )
    .unwrap()
}
fn retire(secret: u128, revision: u64) -> Request {
    Request::new(
        target(),
        Operation::Retire {
            secret: ClientSecretId::from_u128(secret).unwrap(),
            revision,
        },
        Some("Credential retirement fixture"),
    )
    .unwrap()
}
async fn run(
    store: &impl Store<Request = Request, Outcome = Outcome>,
    email: &str,
    request: Request,
) -> Result<Outcome, Error> {
    operator_accounts::run(
        store,
        &Allow,
        &Password(true),
        OperationId::from_u128(Uuid::new_v4().as_u128()).unwrap(),
        request,
        email,
        "source-only passphrase",
    )
    .await
}
async fn fixture() -> Database {
    let db = oidc::fixture().await;
    let overlap: i64 = sqlx::query_scalar(
        "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint+120000",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    for n in 1..=27u128 {
        sqlx::query("INSERT INTO oauth_client_secrets(id,client_id,verifier,created_ms,expires_ms,retired) VALUES($1,$2,$3,0,$4,$5)")
   .bind(Uuid::from_u128(100+n)).bind(Uuid::from_u128(32)).bind([n as u8;32].as_slice()).bind(match n { 2 => Some(overlap), 3 => Some(1), _ => None }).bind(n>=3).execute(&db.pool).await.unwrap();
    }
    db
}
async fn state(db: &Database) -> (String, i64) {
    let state=sqlx::query_scalar("SELECT jsonb_build_object('clients',(SELECT jsonb_agg(c ORDER BY id) FROM oauth_clients c),'secrets',(SELECT jsonb_agg(s ORDER BY id) FROM oauth_client_secrets s),'registration',(SELECT count(*) FROM registration_audit))::text").fetch_one(&db.pool).await.unwrap();
    let audits = sqlx::query_scalar("SELECT count(*) FROM operator_client_secret_audit")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    (state, audits)
}
fn page(out: Outcome) -> Inventory {
    match out {
        Outcome::Listed(p) => p,
        _ => panic!("inventory"),
    }
}
fn command(secret: u128, revision: u64) -> Command {
    Command::RetireSecret {
        application: target().application,
        client: target().client,
        secret: ClientSecretId::from_u128(secret).unwrap(),
        revision,
    }
}
fn prepared() -> Prepared {
    Prepared {
        identifier: None,
        secret: None,
    }
}
#[tokio::test]
async fn inventory_pages_all_history_without_skipping_probe_or_crossing_client_scope() {
    let db = fixture().await;
    let store = db.store.operator_client_secrets();
    let first = page(
        run(&store, "one@example.com", list(None, 25))
            .await
            .unwrap(),
    );
    assert_eq!(first.items.len(), 25);
    assert_eq!(first.next.unwrap().as_u128(), 125);
    assert_eq!(first.revision, 0);
    assert!(first.observed_ms > 1);
    assert_eq!(first.items[2].expires_ms, Some(1));
    assert!(first.items[3].retired);
    let last = page(
        run(&store, "one@example.com", list(Some(125), 25))
            .await
            .unwrap(),
    );
    assert_eq!(
        last.items
            .iter()
            .map(|s| s.id.as_u128())
            .collect::<Vec<_>>(),
        vec![126, 127]
    );
    assert_eq!(last.next, None);
    let empty = page(
        run(&store, "one@example.com", list(Some(999), 1))
            .await
            .unwrap(),
    );
    assert!(empty.items.is_empty());
    assert_eq!(empty.next, None);
    let audits: Vec<String> = sqlx::query_scalar(
        "SELECT row_to_json(a)::text FROM operator_client_secret_audit a ORDER BY id",
    )
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(audits.len(), 3);
    for audit in audits {
        assert!(!audit.contains("verifier"));
        assert!(!audit.contains("source-only"));
        assert!(!audit.contains("@example.com"));
        assert!(audit.contains("client.secret.list"));
    }
}
#[tokio::test]
async fn retirement_shares_http_revision_and_immediately_rejects_only_retired_client_secret() {
    let db = fixture().await;
    let store = db.store.operator_client_secrets();
    assert!(
        db.store
            .authenticate_client(target().client, [1; 32])
            .await
            .is_ok()
    );
    assert_eq!(
        run(&store, "one@example.com", retire(101, 0)).await,
        Ok(Outcome::Retired { revision: 1 })
    );
    assert!(
        db.store
            .authenticate_client(target().client, [1; 32])
            .await
            .is_err()
    );
    assert!(
        db.store
            .authenticate_client(target().client, [2; 32])
            .await
            .is_ok()
    );
    assert_eq!(
        run(&store, "one@example.com", retire(102, 0)).await,
        Err(Error::Conflict)
    );
    assert_eq!(
        run(&store, "one@example.com", retire(101, 1)).await,
        Err(Error::NotFound)
    );
    RegistrationStore::execute(&db.store, [1; 32], &command(102, 1), prepared())
        .await
        .unwrap();
    sqlx::query("INSERT INTO oauth_client_secrets(id,client_id,verifier,created_ms,expires_ms) VALUES($1,$2,$3,0,1)").bind(Uuid::from_u128(128)).bind(Uuid::from_u128(32)).bind([28u8;32].as_slice()).execute(&db.pool).await.unwrap();
    assert_eq!(
        run(&store, "one@example.com", retire(128, 2)).await,
        Ok(Outcome::Retired { revision: 3 })
    ); // expired credentials can be terminally retired
    let current = page(run(&store, "one@example.com", list(None, 4)).await.unwrap());
    assert_eq!(current.revision, 3);
    assert!(current.items.iter().all(|s| s.retired));
    assert!(
        sqlx::query("UPDATE oauth_client_secrets SET retired=false WHERE id=$1")
            .bind(Uuid::from_u128(101))
            .execute(&db.pool)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM registration_audit")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        3
    );
    assert!(
        sqlx::query("UPDATE operator_client_secret_audit SET reason='tampered'")
            .execute(&db.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM operator_client_secret_audit")
            .execute(&db.pool)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn both_operations_require_platform_authority_and_scoped_targets() {
    let db = fixture().await;
    insert_principal(&db, 2, false).await;
    insert_password(&db, 2).await;
    sqlx::query("UPDATE applications SET owner_id=$1,revision=revision+1")
        .bind(Uuid::from_u128(2))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO applications(id,name,owner_id,active) VALUES('00000000-0000-0000-0000-000000000050','Foreign','00000000-0000-0000-0000-000000000001',true); INSERT INTO oauth_clients(id,application_id,name,active) VALUES('00000000-0000-0000-0000-000000000051','00000000-0000-0000-0000-000000000050','Foreign',true); INSERT INTO oauth_client_secrets(id,client_id,verifier,created_ms) VALUES('00000000-0000-0000-0000-000000000052','00000000-0000-0000-0000-000000000051',decode(repeat('ff',32),'hex'),0);").execute(&db.pool).await.unwrap();
    assert_eq!(
        run(
            &db.store.operator_client_secrets(),
            "one@example.com",
            retire(82, 0)
        )
        .await,
        Err(Error::NotFound)
    );
    for request in [list(None, 25), retire(101, 0)] {
        for email in ["person2@example.com", "missing@example.com"] {
            let before = state(&db).await;
            assert_eq!(
                run(&db.store.operator_client_secrets(), email, request.clone()).await,
                Err(Error::Denied)
            );
            let after = state(&db).await;
            assert_eq!(before.0, after.0);
            assert_eq!(after.1, before.1 + 1);
        }
        for scope in [
            Target {
                application: ApplicationId::from_u128(80).unwrap(),
                ..target()
            },
            Target {
                client: ClientId::from_u128(81).unwrap(),
                ..target()
            },
        ] {
            let foreign = Request::new(scope, request.operation(), request.reason()).unwrap();
            assert_eq!(
                run(
                    &db.store.operator_client_secrets(),
                    "one@example.com",
                    foreign
                )
                .await,
                Err(Error::NotFound)
            );
        }
    }
    assert_eq!(
        run(
            &db.store.operator_client_secrets(),
            "one@example.com",
            retire(999, 0)
        )
        .await,
        Err(Error::NotFound)
    );
    sqlx::query("ALTER TABLE oauth_clients DISABLE TRIGGER USER")
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE oauth_clients SET revision=$1")
        .bind(i64::MAX)
        .execute(&db.pool)
        .await
        .unwrap();
    let before = state(&db).await;
    assert_eq!(
        run(
            &db.store.operator_client_secrets(),
            "one@example.com",
            retire(101, i64::MAX as u64)
        )
        .await,
        Err(Error::Conflict)
    );
    assert_eq!(state(&db).await.0, before.0);
}
#[tokio::test]
async fn concurrent_http_and_cli_retirement_have_one_revision_winner() {
    let db = fixture().await;
    let store = db.store.operator_client_secrets();
    let command = command(101, 0);
    let (a, b) = tokio::join!(
        run(&store, "one@example.com", retire(101, 0)),
        RegistrationStore::execute(&db.store, [1; 32], &command, prepared())
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(matches!(a, Ok(_) | Err(Error::Conflict)));
    assert!(matches!(b, Ok(_) | Err(RegistrationError::Conflict)));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT revision FROM oauth_clients")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
}
#[tokio::test]
async fn suppressed_or_failed_credential_parent_and_audit_writes_roll_back_everything() {
    for (table, event) in [
        ("oauth_client_secrets", "UPDATE"),
        ("oauth_clients", "UPDATE"),
        ("registration_audit", "INSERT"),
        ("operator_client_secret_audit", "INSERT"),
    ] {
        for body in ["RETURN NULL;", "RAISE EXCEPTION 'fixture';"] {
            let db = fixture().await;
            let before = state(&db).await;
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION reject_retirement() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN {body} END $$; CREATE TRIGGER reject_retirement BEFORE {event} ON {table} FOR EACH ROW EXECUTE FUNCTION reject_retirement();"))).execute(&db.pool).await.unwrap();
            assert_eq!(
                run(
                    &db.store.operator_client_secrets(),
                    "one@example.com",
                    retire(101, 0)
                )
                .await,
                Err(Error::Unavailable),
                "{table} {body}"
            );
            assert_eq!(state(&db).await, before);
            if table == "operator_client_secret_audit" {
                for (email, request) in [
                    ("one@example.com", list(None, 1)),
                    ("missing@example.com", list(None, 1)),
                    ("missing@example.com", retire(101, 0)),
                ] {
                    assert_eq!(
                        run(&db.store.operator_client_secrets(), email, request).await,
                        Err(Error::Unavailable)
                    );
                }
            } else {
                assert!(
                    RegistrationStore::execute(&db.store, [1; 32], &command(101, 0), prepared())
                        .await
                        .is_err()
                );
            }
            assert_eq!(state(&db).await, before);
        }
    }
}
#[tokio::test]
async fn inventory_and_retirement_recheck_authority_after_fence_waits() {
    for request in [list(None, 1), retire(101, 0)] {
        for age in [0, 60_000] {
            let db = fixture().await;
            insert_principal(&db, 3, true).await;
            let held = Held {
                inner: db.store.operator_client_secrets(),
                ready: tokio::sync::Notify::new(),
                resume: tokio::sync::Notify::new(),
                age,
            };
            let mut tx = db.pool.begin().await.unwrap();
            sqlx::query("SELECT singleton FROM security_state FOR UPDATE")
                .execute(&mut *tx)
                .await
                .unwrap();
            let reduce = async {
                held.ready.notified().await;
                held.resume.notify_one();
                tokio::time::timeout(std::time::Duration::from_secs(5),async{loop{
    let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE '%security_state%')").fetch_one(&db.pool).await.unwrap();if waiting{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;
   }}).await.unwrap();
                if age == 0 {
                    sqlx::query("DELETE FROM platform_administrators WHERE principal_id=$1")
                        .bind(Uuid::from_u128(1))
                        .execute(&mut *tx)
                        .await
                        .unwrap();
                }
                tx.commit().await.unwrap();
            };
            let (result, ()) = tokio::join!(run(&held, "one@example.com", request.clone()), reduce);
            assert_eq!(result, Err(Error::Denied));
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT revision FROM oauth_clients")
                    .fetch_one(&db.pool)
                    .await
                    .unwrap(),
                0
            );
        }
    }
}
#[tokio::test]
async fn late_actor_reductions_never_disclose_inventory_or_commit_retirement() {
    for (table, event, request) in [
        ("oauth_client_secrets", "UPDATE", retire(101, 0)),
        ("operator_client_secret_audit", "INSERT", retire(101, 0)),
        ("operator_client_secret_audit", "INSERT", list(None, 1)),
    ] {
        let db = fixture().await;
        let before = state(&db).await;
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION revoke_secret_actor() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE credentials SET revoked=true WHERE principal_id='00000000-0000-0000-0000-000000000001'; RETURN NEW; END $$; CREATE TRIGGER revoke_secret_actor AFTER {event} ON {table} FOR EACH ROW EXECUTE FUNCTION revoke_secret_actor();"))).execute(&db.pool).await.unwrap();
        assert_eq!(
            run(
                &db.store.operator_client_secrets(),
                "one@example.com",
                request
            )
            .await,
            Err(Error::Denied)
        );
        let after = state(&db).await;
        assert_eq!(after.0, before.0);
        assert_eq!(
            after.1,
            before.1
                + if table == "oauth_client_secrets" {
                    1
                } else {
                    0
                }
        );
    }
}
#[tokio::test]
async fn failed_and_lost_commit_acknowledgements_never_retry_reads_or_retirement() {
    for request in [list(None, 1), retire(101, 0)] {
        let db = fixture().await;
        let before = state(&db).await;
        sqlx::raw_sql("CREATE FUNCTION reject_secret_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture'; END $$; CREATE CONSTRAINT TRIGGER reject_secret_commit AFTER INSERT ON operator_client_secret_audit DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_secret_commit();").execute(&db.pool).await.unwrap();
        assert_eq!(
            run(
                &db.store.operator_client_secrets(),
                "one@example.com",
                request.clone()
            )
            .await,
            Err(Error::Uncertain)
        );
        assert_eq!(state(&db).await, before);
        sqlx::query("DROP TRIGGER reject_secret_commit ON operator_client_secret_audit")
            .execute(&db.pool)
            .await
            .unwrap();
        let (store, proxy) = super::limiter_activation::lost_nth_commit(&db.pool, 1).await;
        assert_eq!(
            run(
                &store.operator_client_secrets(),
                "one@example.com",
                request.clone()
            )
            .await,
            Err(Error::Uncertain)
        );
        proxy.await.unwrap();
        store.close().await;
        assert_eq!(state(&db).await.1, before.1 + 1);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT revision FROM oauth_clients")
                .fetch_one(&db.pool)
                .await
                .unwrap(),
            if matches!(request.operation(), Operation::Retire { .. }) {
                1
            } else {
                0
            }
        );
    }
}
#[tokio::test]
async fn migration_thirty_preserves_credentials_and_previous_operator_audits() {
    let db = Database::at_version(29).await;
    insert_principal(&db, 1, true).await;
    sqlx::raw_sql("INSERT INTO operator_client_audit(operation_id,application_id,client_id,expected_revision,reason,result,occurred_ms) VALUES('00000000-0000-0000-0000-000000000002','00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000020',0,'Historical attempt','denied',1); INSERT INTO applications(id,name,owner_id,active) VALUES('00000000-0000-0000-0000-000000000010','Historical app','00000000-0000-0000-0000-000000000001',true); INSERT INTO oauth_clients(id,application_id,name,active) VALUES('00000000-0000-0000-0000-000000000020','00000000-0000-0000-0000-000000000010','Historical client',true); INSERT INTO oauth_client_secrets(id,client_id,verifier,created_ms) VALUES('00000000-0000-0000-0000-000000000003','00000000-0000-0000-0000-000000000020',decode(repeat('11',32),'hex'),0);").execute(&db.pool).await.unwrap();
    let before:String=sqlx::query_scalar("SELECT jsonb_build_object('audit',(SELECT row_to_json(a) FROM operator_client_audit a),'secret',(SELECT row_to_json(s) FROM oauth_client_secrets s))::text").fetch_one(&db.pool).await.unwrap();
    db.store
        .migrate_operation(OperationId::from_u128(3).unwrap())
        .await
        .unwrap();
    assert_eq!(before,sqlx::query_scalar::<_,String>("SELECT jsonb_build_object('audit',(SELECT row_to_json(a) FROM operator_client_audit a),'secret',(SELECT row_to_json(s) FROM oauth_client_secrets s))::text").fetch_one(&db.pool).await.unwrap());
    assert_eq!(state(&db).await.1, 0);
}

#[tokio::test]
async fn audit_constraints_reject_incoherent_actor_query_and_retirement_facts() {
    let db = fixture().await;
    run(
        &db.store.operator_client_secrets(),
        "one@example.com",
        list(None, 1),
    )
    .await
    .unwrap();
    run(
        &db.store.operator_client_secrets(),
        "one@example.com",
        retire(101, 0),
    )
    .await
    .unwrap();
    let rows: Vec<serde_json::Value> =
        sqlx::query_scalar("SELECT to_jsonb(a) FROM operator_client_secret_audit a ORDER BY id")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    for (index, patch) in [
        (0, serde_json::json!({"actor_id":null})),
        (0, serde_json::json!({"actor_credential_id":null})),
        (0, serde_json::json!({"actor_epoch":-1})),
        (0, serde_json::json!({"authentication_observed_ms":-1})),
        (
            0,
            serde_json::json!({"client_id":"00000000-0000-0000-0000-000000000000"}),
        ),
        (0, serde_json::json!({"page_size":26})),
        (0, serde_json::json!({"returned_count":2})),
        (0, serde_json::json!({"returned_count":null})),
        (0, serde_json::json!({"target_revision":null})),
        (0, serde_json::json!({"reason":"unexpected"})),
        (0, serde_json::json!({"result":"written"})),
        (1, serde_json::json!({"secret_id":null})),
        (1, serde_json::json!({"expected_revision":null})),
        (1, serde_json::json!({"reason":null})),
        (1, serde_json::json!({"reason":"line\nbreak"})),
        (1, serde_json::json!({"page_size":1})),
        (1, serde_json::json!({"target_revision":3})),
        (
            1,
            serde_json::json!({"cursor_id":"00000000-0000-0000-0000-000000000001"}),
        ),
        (1, serde_json::json!({"result":"uncertain"})),
    ] {
        let mut row = rows[index].clone();
        for (key, value) in patch.as_object().unwrap() {
            row[key] = value.clone();
        }
        row["operation_id"] = Uuid::new_v4().to_string().into();
        assert!(sqlx::query("INSERT INTO operator_client_secret_audit(operation_id,command,application_id,client_id,secret_id,expected_revision,target_revision,cursor_id,page_size,returned_count,reason,actor_id,actor_credential_id,actor_epoch,authentication_observed_ms,result,occurred_ms) SELECT operation_id,command,application_id,client_id,secret_id,expected_revision,target_revision,cursor_id,page_size,returned_count,reason,actor_id,actor_credential_id,actor_epoch,authentication_observed_ms,result,occurred_ms FROM jsonb_populate_record(NULL::operator_client_secret_audit,$1)").bind(row).execute(&db.pool).await.is_err(),"{patch}");
    }
    assert_eq!(state(&db).await.1, 2);
}
