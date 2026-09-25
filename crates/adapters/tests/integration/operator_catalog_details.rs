use super::operator_directory::{Allow, Held, Password};
use super::*;
use darkhorse_application::{
    operator_accounts::{self, Store},
    registration::{ReadTarget, Record, RegistrationStore},
};
use darkhorse_domain::{
    identity::{ApplicationId, ClientId, OperationId},
    operator_accounts::Error,
};
fn target() -> ReadTarget {
    ReadTarget::Client {
        application: ApplicationId::from_u128(16).unwrap(),
        client: ClientId::from_u128(32).unwrap(),
    }
}
async fn show(
    store: &impl Store<Request = ReadTarget, Outcome = Record>,
    email: &str,
    target: ReadTarget,
) -> Result<Record, Error> {
    operator_accounts::run(
        store,
        &Allow,
        &Password(true),
        OperationId::from_u128(Uuid::new_v4().as_u128()).unwrap(),
        target,
        email,
        "source-only passphrase",
    )
    .await
}
fn detail_targets() -> [ReadTarget; 4] {
    [
        ReadTarget::Application(ApplicationId::from_u128(16).unwrap()),
        target(),
        ReadTarget::Application(ApplicationId::from_u128(999).unwrap()),
        ReadTarget::Client {
            application: ApplicationId::from_u128(16).unwrap(),
            client: ClientId::from_u128(999).unwrap(),
        },
    ]
}
#[tokio::test]
async fn detail_audit_time_authority_loss_rolls_back_without_disclosing_target_existence() {
    for change in [
        "DELETE FROM platform_administrators WHERE principal_id=NEW.actor_id;",
        "UPDATE credentials SET revoked=true WHERE id=NEW.actor_credential_id;",
        "UPDATE principals SET credential_epoch=credential_epoch+1,revision=revision+1 WHERE id=NEW.actor_id;",
        "UPDATE principals SET active=false,credential_epoch=credential_epoch+1,revision=revision+1 WHERE id=NEW.actor_id;",
    ] {
        let db = oidc::fixture().await;
        insert_principal(&db, 3, true).await;
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "CREATE FUNCTION reduce_detail_actor() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN {change} RETURN NEW; END $$; CREATE TRIGGER reduce_detail_actor AFTER INSERT ON operator_catalog_detail_audit FOR EACH ROW EXECUTE FUNCTION reduce_detail_actor();"
        )))
        .execute(&db.pool)
        .await
        .unwrap();
        for target in detail_targets() {
            assert!(
                matches!(
                    show(
                        &db.store.operator_catalog_details(),
                        "one@example.com",
                        target
                    )
                    .await,
                    Err(Error::Denied)
                ),
                "audit-time reduction must deny existing and missing targets: {change}"
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM operator_catalog_detail_audit")
                    .fetch_one(&db.pool)
                    .await
                    .unwrap(),
                0
            );
            assert!(sqlx::query_scalar::<_, bool>("SELECT p.active AND p.credential_epoch=0 AND NOT c.revoked AND EXISTS(SELECT 1 FROM platform_administrators WHERE principal_id=p.id) FROM principals p JOIN credentials c ON c.principal_id=p.id WHERE p.id='00000000-0000-0000-0000-000000000001'").fetch_one(&db.pool).await.unwrap());
        }
        // A rollback must leave the credential usable once the injected fault is removed.
        sqlx::query("DROP TRIGGER reduce_detail_actor ON operator_catalog_detail_audit")
            .execute(&db.pool)
            .await
            .unwrap();
        assert!(
            show(
                &db.store.operator_catalog_details(),
                "one@example.com",
                target()
            )
            .await
            .is_ok()
        );
    }
}
#[tokio::test]
async fn detail_proof_expiring_during_audit_releases_neither_record_nor_not_found() {
    let db = oidc::fixture().await;
    // The sequence survives rollback and proves the successful/missing read reached its audit.
    sqlx::raw_sql("CREATE SEQUENCE detail_audit_reached; CREATE FUNCTION expire_detail_proof() RETURNS trigger LANGUAGE plpgsql AS $$ DECLARE remaining double precision; BEGIN remaining := (NEW.authentication_observed_ms+60000)/1000.0-extract(epoch FROM clock_timestamp()); IF NEW.result NOT IN ('read','not_found') OR remaining<=0 THEN RAISE EXCEPTION 'fixture did not reach audit with a live proof'; END IF; PERFORM nextval('detail_audit_reached'); PERFORM pg_sleep(remaining+0.025); RETURN NEW; END $$; CREATE TRIGGER expire_detail_proof BEFORE INSERT ON operator_catalog_detail_audit FOR EACH ROW EXECUTE FUNCTION expire_detail_proof();").execute(&db.pool).await.unwrap();
    for (index, target) in detail_targets().into_iter().enumerate() {
        let held = Held {
            inner: db.store.operator_catalog_details(),
            ready: tokio::sync::Notify::new(),
            resume: tokio::sync::Notify::new(),
            age: 58_000,
        };
        let resume = async {
            held.ready.notified().await;
            held.resume.notify_one();
        };
        let (result, ()) = tokio::join!(show(&held, "one@example.com", target), resume);
        assert!(matches!(result, Err(Error::Denied)));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT last_value FROM detail_audit_reached")
                .fetch_one(&db.pool)
                .await
                .unwrap(),
            index as i64 + 1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM operator_catalog_detail_audit")
                .fetch_one(&db.pool)
                .await
                .unwrap(),
            0
        );
    }
}
#[tokio::test]
async fn details_reuse_http_configuration_without_reading_secret_metadata_or_changing_state() {
    let db = oidc::fixture().await;
    sqlx::raw_sql("INSERT INTO oauth_client_secrets(id,client_id,verifier,created_ms) VALUES('00000000-0000-0000-0000-000000000099','00000000-0000-0000-0000-000000000020',decode(repeat('ab',32),'hex'),0); UPDATE oauth_clients SET active=false,refresh_tokens=true,revision=revision+1; UPDATE applications SET active=false,revision=revision+1;").execute(&db.pool).await.unwrap();
    for target in [
        ReadTarget::Application(ApplicationId::from_u128(16).unwrap()),
        target(),
    ] {
        let cli = show(
            &db.store.operator_catalog_details(),
            "one@example.com",
            target,
        )
        .await
        .unwrap();
        let http = RegistrationStore::read(&db.store, [1; 32], target)
            .await
            .unwrap();
        match (cli, http) {
            (Record::Application(a), Record::Application(b)) => {
                assert_eq!(a.id, b.id);
                assert_eq!(a.owner_email, b.owner_email);
                assert_eq!(a.name, b.name);
                assert_eq!(a.revision, b.revision);
                assert!(!a.active);
            }
            (Record::Client(a), Record::Client(b)) => {
                assert_eq!(a.id, b.id);
                assert_eq!(a.application, b.application);
                assert_eq!(a.revision, b.revision);
                assert_eq!(a.spec.redirects, b.spec.redirects);
                assert_eq!(a.spec.resources, b.spec.resources);
                assert_eq!(a.spec.scopes, b.spec.scopes);
                assert!(a.spec.refresh_tokens);
                assert!(!a.spec.active);
                assert!(a.secrets.is_empty());
                assert_eq!(b.secrets.len(), 1);
            }
            _ => panic!("mismatched configuration"),
        }
    }
    let audit: Vec<String> =
        sqlx::query_scalar("SELECT row_to_json(a)::text FROM operator_catalog_detail_audit a")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(audit.len(), 2);
    assert!(audit.iter().all(|a| !a.contains("callback")
        && !a.contains("one@example.com")
        && !a.contains("ababab")));
}
#[tokio::test]
async fn foreign_missing_and_owner_only_detail_reads_are_denied_or_audited_not_found() {
    let db = oidc::fixture().await;
    insert_principal(&db, 2, false).await;
    insert_password(&db, 2).await;
    sqlx::query("UPDATE applications SET owner_id=$1,revision=revision+1")
        .bind(Uuid::from_u128(2))
        .execute(&db.pool)
        .await
        .unwrap();
    for email in ["person2@example.com", "missing@example.com"] {
        assert!(matches!(
            show(&db.store.operator_catalog_details(), email, target()).await,
            Err(Error::Denied)
        ));
    }
    for missing in [
        ReadTarget::Application(ApplicationId::from_u128(999).unwrap()),
        ReadTarget::Client {
            application: ApplicationId::from_u128(999).unwrap(),
            client: ClientId::from_u128(32).unwrap(),
        },
        ReadTarget::Client {
            application: ApplicationId::from_u128(16).unwrap(),
            client: ClientId::from_u128(999).unwrap(),
        },
    ] {
        assert!(matches!(
            show(
                &db.store.operator_catalog_details(),
                "one@example.com",
                missing
            )
            .await,
            Err(Error::NotFound)
        ));
    }
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM operator_catalog_detail_audit WHERE result='not_found' AND actor_id IS NOT NULL").fetch_one(&db.pool).await.unwrap(),3);
}
#[tokio::test]
async fn detail_reader_waits_for_security_writers_and_rechecks_demotion_and_proof_age() {
    for age in [0, 60_000] {
        let db = oidc::fixture().await;
        insert_principal(&db, 3, true).await;
        let held = Held {
            inner: db.store.operator_catalog_details(),
            ready: tokio::sync::Notify::new(),
            resume: tokio::sync::Notify::new(),
            age,
        };
        let mut tx = db.pool.begin().await.unwrap();
        sqlx::query("SELECT singleton FROM security_state FOR UPDATE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let request = show(&held, "one@example.com", target());
        let reduce = async {
            held.ready.notified().await;
            held.resume.notify_one();
            tokio::time::timeout(std::time::Duration::from_secs(5),async {
                loop {
                    let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE '%security_state%')").fetch_one(&db.pool).await.unwrap();
                    if waiting {break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            }).await.unwrap();
            if age == 0 {
                sqlx::query("DELETE FROM platform_administrators WHERE principal_id=$1")
                    .bind(Uuid::from_u128(1))
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            tx.commit().await.unwrap();
        };
        let (result, ()) = tokio::join!(request, reduce);
        assert!(matches!(result, Err(Error::Denied)));
    }
}
#[tokio::test]
async fn detail_audit_rejection_or_suppression_releases_no_configuration() {
    for body in ["RAISE EXCEPTION 'fixture';", "RETURN NULL;"] {
        let db = oidc::fixture().await;
        let sql = format!(
            "CREATE FUNCTION reject_detail() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN {body} END $$; CREATE TRIGGER reject_detail BEFORE INSERT ON operator_catalog_detail_audit FOR EACH ROW EXECUTE FUNCTION reject_detail();"
        );
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
            .execute(&db.pool)
            .await
            .unwrap();
        for email in ["one@example.com", "missing@example.com"] {
            assert!(matches!(
                show(&db.store.operator_catalog_details(), email, target()).await,
                Err(Error::Unavailable)
            ));
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM operator_catalog_detail_audit")
                .fetch_one(&db.pool)
                .await
                .unwrap(),
            0
        );
    }
}
#[tokio::test]
async fn detail_commit_failure_and_lost_acknowledgement_never_return_configuration_or_retry() {
    let db = oidc::fixture().await;
    sqlx::raw_sql("CREATE FUNCTION reject_detail_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture'; END $$; CREATE CONSTRAINT TRIGGER reject_detail_commit AFTER INSERT ON operator_catalog_detail_audit DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_detail_commit();").execute(&db.pool).await.unwrap();
    assert!(matches!(
        show(
            &db.store.operator_catalog_details(),
            "one@example.com",
            target()
        )
        .await,
        Err(Error::Uncertain)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM operator_catalog_detail_audit")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query("DROP TRIGGER reject_detail_commit ON operator_catalog_detail_audit")
        .execute(&db.pool)
        .await
        .unwrap();
    let (store, proxy) = super::limiter_activation::lost_nth_commit(&db.pool, 1).await;
    assert!(matches!(
        show(
            &store.operator_catalog_details(),
            "one@example.com",
            target()
        )
        .await,
        Err(Error::Uncertain)
    ));
    proxy.await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM operator_catalog_detail_audit WHERE result='read'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        1
    );
    store.close().await;
}
#[tokio::test]
async fn stored_callbacks_and_allowances_over_the_contract_are_rejected_instead_of_truncated() {
    for sql in [
        "INSERT INTO client_redirects(client_id,uri) SELECT '00000000-0000-0000-0000-000000000020','https://client.example/'||n FROM generate_series(1,8)n",
        "INSERT INTO protected_resources(id,application_id,name,audience) SELECT lpad(to_hex(n),32,'0')::uuid,'00000000-0000-0000-0000-000000000010','API','urn:darkhorse:resource:'||(lpad(to_hex(n),32,'0')::uuid)::text FROM generate_series(1000,1032)n; INSERT INTO client_resources(application_id,client_id,resource_id) SELECT application_id,'00000000-0000-0000-0000-000000000020',id FROM protected_resources",
        "INSERT INTO protected_resources(id,application_id,name,audience) VALUES('00000000-0000-0000-0000-000000000030','00000000-0000-0000-0000-000000000010','API','urn:darkhorse:resource:00000000-0000-0000-0000-000000000030'); INSERT INTO client_resources VALUES('00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000020','00000000-0000-0000-0000-000000000030'); INSERT INTO resource_scopes(id,application_id,resource_id,name) SELECT lpad(to_hex(n),32,'0')::uuid,'00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000030','scope'||n FROM generate_series(1000,1128)n; INSERT INTO client_scopes(application_id,client_id,resource_id,scope_id) SELECT application_id,'00000000-0000-0000-0000-000000000020',resource_id,id FROM resource_scopes",
    ] {
        let db = oidc::fixture().await;
        sqlx::raw_sql(sql).execute(&db.pool).await.unwrap();
        assert!(matches!(
            show(
                &db.store.operator_catalog_details(),
                "one@example.com",
                target()
            )
            .await,
            Err(Error::Unavailable)
        ));
        assert!(
            RegistrationStore::read(&db.store, [1; 32], target())
                .await
                .is_err()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM operator_catalog_detail_audit")
                .fetch_one(&db.pool)
                .await
                .unwrap(),
            0
        );
    }
}

#[tokio::test]
async fn detail_migration_preserves_catalog_and_historical_listing_audit() {
    let db = Database::at_version(26).await;
    insert_principal(&db, 1, true).await;
    sqlx::raw_sql("INSERT INTO applications(id,name,owner_id,active) VALUES('00000000-0000-0000-0000-000000000010','Before upgrade','00000000-0000-0000-0000-000000000001',true); INSERT INTO operator_catalog_audit(operation_id,command,query_limit,searched,result,occurred_ms) VALUES('00000000-0000-0000-0000-000000000002','application.list',25,false,'denied',1)").execute(&db.pool).await.unwrap();
    let before: String =
        sqlx::query_scalar("SELECT row_to_json(a)::text FROM operator_catalog_audit a")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    db.store
        .migrate_operation(OperationId::from_u128(3).unwrap())
        .await
        .unwrap();
    let after: String =
        sqlx::query_scalar("SELECT row_to_json(a)::text FROM operator_catalog_audit a")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT name FROM applications")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        "Before upgrade"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM operator_catalog_detail_audit")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    assert!(
        sqlx::query("UPDATE operator_catalog_audit SET result='denied'")
            .execute(&db.pool)
            .await
            .is_err()
    );
    db.pool.close().await;
}
