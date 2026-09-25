use super::operator_directory::{Allow, Held, Password};
use super::*;
use darkhorse_application::{
    operator_accounts::{self, Store},
    registration::{ApplicationRecord, Entropy, NewSecret, Prepared, Record, RegistrationStore},
};
use darkhorse_domain::{
    identity::{ApplicationId, OperationId, PrincipalId},
    operator_accounts::Error,
    operator_applications::{Operation, Request},
    registration::{ApplicationSpec, Label, RegistrationError},
};
use std::{
    num::NonZeroU128,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
#[tokio::test]
async fn target_dependent_application_errors_recheck_authority_after_audit() {
    use super::operator_authority::{REDUCTIONS, inject, untouched};
    for reduction in REDUCTIONS {
        let db = oidc::fixture().await;
        insert_principal(&db, 3, true).await;
        inject(&db, "operator_application_audit", reduction).await;
        for operation in [
            update(99, 1, true),
            update(0, 999, true),
            Operation::Update {
                application: ApplicationId::from_u128(999).unwrap(),
                revision: 0,
                spec: spec(1, true),
            },
        ] {
            assert!(matches!(
                write(
                    &db.store.operator_applications(Material::default()),
                    "one@example.com",
                    operation
                )
                .await,
                Err(Error::Denied)
            ));
            untouched(&db, "operator_application_audit").await;
        }
    }
}
#[tokio::test]
async fn conflict_with_proof_expiring_during_application_audit_is_denied() {
    use super::operator_authority::{Aged, expiry, untouched};
    let db = oidc::fixture().await;
    sqlx::query("CREATE SEQUENCE authority_audit_reached")
        .execute(&db.pool)
        .await
        .unwrap();
    expiry(&db, "operator_application_audit").await;
    assert!(matches!(
        write(
            &Aged(db.store.operator_applications(Material::default())),
            "one@example.com",
            update(99, 1, true)
        )
        .await,
        Err(Error::Denied)
    ));
    untouched(&db, "operator_application_audit").await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT last_value FROM authority_audit_reached")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
}
#[derive(Clone, Default)]
struct Material(Arc<AtomicUsize>);
impl Entropy for Material {
    fn identifier(&self) -> Result<NonZeroU128, RegistrationError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(NonZeroU128::new(100).unwrap())
    }
    fn secret(&self) -> Result<NewSecret, RegistrationError> {
        panic!("application commands never issue secrets")
    }
}
fn spec(owner: u128, active: bool) -> ApplicationSpec {
    ApplicationSpec {
        name: Label::new("Private application name").unwrap(),
        owner: PrincipalId::from_u128(owner).unwrap(),
        active,
    }
}
fn update(revision: u64, owner: u128, active: bool) -> Operation {
    Operation::Update {
        application: ApplicationId::from_u128(16).unwrap(),
        revision,
        spec: spec(owner, active),
    }
}
async fn write(
    store: &impl Store<Request = Request, Outcome = ApplicationRecord>,
    email: &str,
    operation: Operation,
) -> Result<ApplicationRecord, Error> {
    operator_accounts::run(
        store,
        &Allow,
        &Password(true),
        OperationId::from_u128(Uuid::new_v4().as_u128()).unwrap(),
        Request::new(operation, "Approved fixture change").unwrap(),
        email,
        "source-only passphrase",
    )
    .await
}
async fn state(db: &Database) -> (String, i64, i64, i64) {
    let applications: String = sqlx::query_scalar(
        "SELECT coalesce(json_agg(a ORDER BY id)::text,'[]') FROM applications a",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let registration: i64 = sqlx::query_scalar("SELECT count(*) FROM registration_audit")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let audit: i64 = sqlx::query_scalar("SELECT count(*) FROM operator_application_audit")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let policy: i64 = sqlx::query_scalar("SELECT policy_revision FROM security_state")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    (applications, registration, audit, policy)
}
#[tokio::test]
async fn create_and_complete_updates_share_http_policy_and_commit_both_audits() {
    let db = oidc::fixture().await;
    insert_principal(&db, 2, false).await;
    let material = Material::default();
    let store = db.store.operator_applications(material.clone());
    let created = write(&store, "one@example.com", Operation::Create(spec(2, true)))
        .await
        .unwrap();
    assert_eq!(created.id.as_u128(), 100);
    assert_eq!(created.revision, 0);
    let changed = write(&store, "one@example.com", update(0, 2, false))
        .await
        .unwrap();
    assert_eq!(changed.owner.as_u128(), 2);
    assert!(!changed.active);
    assert_eq!(changed.revision, 1);
    assert_eq!(material.0.load(Ordering::SeqCst), 1);
    let command = darkhorse_application::registration::Command::UpdateApplication {
        application: changed.id,
        revision: 1,
        spec: spec(1, true),
    };
    let http = RegistrationStore::execute(
        &db.store,
        [1; 32],
        &command,
        Prepared {
            identifier: None,
            secret: None,
        },
    )
    .await
    .unwrap();
    let Record::Application(http) = http else {
        panic!("application record")
    };
    assert_eq!(http.revision, 2);
    assert!(http.active);
    assert_eq!(http.owner.as_u128(), 1);
    let changed = write(&store, "one@example.com", update(2, 2, false))
        .await
        .unwrap();
    assert_eq!(changed.revision, 3);
    let audits: Vec<String> =
        sqlx::query_scalar("SELECT row_to_json(a)::text FROM operator_application_audit a")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(audits.len(), 3);
    assert!(
        audits
            .iter()
            .all(|a| !a.contains("Private application name")
                && !a.contains("@example.com")
                && !a.contains("source-only passphrase"))
    );
    assert_eq!(state(&db).await.1, 4);
}
#[tokio::test]
async fn invalid_owners_stale_revisions_missing_targets_and_nonadministrators_cannot_write() {
    let db = oidc::fixture().await;
    insert_principal(&db, 2, false).await;
    insert_password(&db, 2).await;
    insert_principal(&db, 3, false).await;
    sqlx::query("UPDATE principals SET active=false,revision=revision+1,credential_epoch=credential_epoch+1 WHERE id=$1").bind(Uuid::from_u128(3)).execute(&db.pool).await.unwrap();
    sqlx::query("UPDATE applications SET owner_id=$1,revision=revision+1")
        .bind(Uuid::from_u128(2))
        .execute(&db.pool)
        .await
        .unwrap();
    let material = Material::default();
    let store = db.store.operator_applications(material.clone());
    for (email, operation, error) in [
        ("person2@example.com", update(1, 2, false), Error::Denied),
        (
            "missing@example.com",
            Operation::Create(spec(1, true)),
            Error::Denied,
        ),
        (
            "one@example.com",
            Operation::Create(spec(999, true)),
            Error::Invalid,
        ),
        (
            "one@example.com",
            Operation::Create(spec(3, true)),
            Error::Invalid,
        ),
        ("one@example.com", update(1, 999, true), Error::Invalid),
        ("one@example.com", update(99, 1, true), Error::Conflict),
        (
            "one@example.com",
            Operation::Update {
                application: ApplicationId::from_u128(999).unwrap(),
                revision: 0,
                spec: spec(1, true),
            },
            Error::NotFound,
        ),
    ] {
        let before = state(&db).await;
        assert!(matches!(write(&store,email,operation).await,Err(actual) if actual==error));
        let after = state(&db).await;
        assert_eq!(
            (&before.0, before.1, before.3),
            (&after.0, after.1, after.3)
        );
        assert_eq!(after.2, before.2 + 1);
    }
    assert_eq!(material.0.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn concurrent_cli_and_http_edits_have_one_winner_at_the_expected_revision() {
    let db = oidc::fixture().await;
    let store = db.store.operator_applications(Material::default());
    let command = darkhorse_application::registration::Command::UpdateApplication {
        application: ApplicationId::from_u128(16).unwrap(),
        revision: 0,
        spec: spec(1, true),
    };
    let (cli, http) = tokio::join!(
        write(&store, "one@example.com", update(0, 1, false)),
        RegistrationStore::execute(
            &db.store,
            [1; 32],
            &command,
            Prepared {
                identifier: None,
                secret: None
            }
        )
    );
    assert_eq!(usize::from(cli.is_ok()) + usize::from(http.is_ok()), 1);
    assert!(matches!(cli, Ok(_) | Err(Error::Conflict)));
    assert!(matches!(http, Ok(_) | Err(RegistrationError::Conflict)));
    assert_eq!(state(&db).await.1, 1);
}
#[tokio::test]
async fn suppression_or_failure_of_application_and_either_audit_rolls_back_every_effect() {
    for table in [
        "applications",
        "registration_audit",
        "operator_application_audit",
    ] {
        for body in ["RAISE EXCEPTION 'fixture';", "RETURN NULL;"] {
            let db = oidc::fixture().await;
            let before = state(&db).await;
            let sql = format!(
                "CREATE FUNCTION reject_write() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN {body} END $$; CREATE TRIGGER reject_write BEFORE INSERT OR UPDATE ON {table} FOR EACH ROW EXECUTE FUNCTION reject_write();"
            );
            sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
                .execute(&db.pool)
                .await
                .unwrap();
            let store = db.store.operator_applications(Material::default());
            for operation in [Operation::Create(spec(1, true)), update(0, 1, false)] {
                assert!(matches!(
                    write(&store, "one@example.com", operation).await,
                    Err(Error::Unavailable)
                ));
                assert_eq!(state(&db).await, before);
            }
            if table == "operator_application_audit" {
                assert!(matches!(
                    write(
                        &store,
                        "missing@example.com",
                        Operation::Create(spec(1, true))
                    )
                    .await,
                    Err(Error::Unavailable)
                ));
                assert_eq!(state(&db).await, before);
            }
            if table != "operator_application_audit" {
                let command = darkhorse_application::registration::Command::UpdateApplication {
                    application: ApplicationId::from_u128(16).unwrap(),
                    revision: 0,
                    spec: spec(1, false),
                };
                assert!(
                    RegistrationStore::execute(
                        &db.store,
                        [1; 32],
                        &command,
                        Prepared {
                            identifier: None,
                            secret: None
                        }
                    )
                    .await
                    .is_err()
                );
                assert_eq!(state(&db).await, before);
            }
        }
    }
}
#[tokio::test]
async fn application_write_rechecks_demotions_and_proof_expiry_after_lock_waits() {
    for age in [0, 60_000] {
        let db = oidc::fixture().await;
        insert_principal(&db, 3, true).await;
        let held = Held {
            inner: db.store.operator_applications(Material::default()),
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
            tokio::time::timeout(std::time::Duration::from_secs(5),async {
                loop {
                    let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE '%security_state%')").fetch_one(&db.pool).await.unwrap();
                    if waiting {break;} tokio::time::sleep(std::time::Duration::from_millis(10)).await;
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
        let (result, ()) =
            tokio::join!(write(&held, "one@example.com", update(0, 1, false)), reduce);
        assert!(matches!(result, Err(Error::Denied)));
        assert_eq!(state(&db).await.1, 0);
    }
}
#[tokio::test]
async fn authority_loss_during_write_rolls_back_mutation_and_registration_audit() {
    let db = oidc::fixture().await;
    sqlx::raw_sql("CREATE FUNCTION revoke_during_write() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE credentials SET revoked=true WHERE principal_id='00000000-0000-0000-0000-000000000001'; RETURN NEW; END $$; CREATE TRIGGER revoke_during_write AFTER UPDATE ON applications FOR EACH ROW EXECUTE FUNCTION revoke_during_write();").execute(&db.pool).await.unwrap();
    let before = state(&db).await;
    assert!(matches!(
        write(
            &db.store.operator_applications(Material::default()),
            "one@example.com",
            update(0, 1, false)
        )
        .await,
        Err(Error::Denied)
    ));
    let after = state(&db).await;
    assert_eq!(
        (&before.0, before.1, before.3),
        (&after.0, after.1, after.3)
    );
    assert_eq!(after.2, before.2 + 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM credentials WHERE revoked")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
}
#[tokio::test]
async fn failed_or_lost_commit_is_uncertain_and_never_retries_application_creation() {
    let db = oidc::fixture().await;
    sqlx::raw_sql("CREATE FUNCTION reject_application_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture'; END $$; CREATE CONSTRAINT TRIGGER reject_application_commit AFTER INSERT ON operator_application_audit DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_application_commit();").execute(&db.pool).await.unwrap();
    let before = state(&db).await;
    assert!(matches!(
        write(
            &db.store.operator_applications(Material::default()),
            "one@example.com",
            Operation::Create(spec(1, true))
        )
        .await,
        Err(Error::Uncertain)
    ));
    assert_eq!(state(&db).await, before);
    sqlx::query("DROP TRIGGER reject_application_commit ON operator_application_audit")
        .execute(&db.pool)
        .await
        .unwrap();
    let (store, proxy) = super::limiter_activation::lost_nth_commit(&db.pool, 1).await;
    let material = Material::default();
    assert!(matches!(
        write(
            &store.operator_applications(material.clone()),
            "one@example.com",
            Operation::Create(spec(1, true))
        )
        .await,
        Err(Error::Uncertain)
    ));
    proxy.await.unwrap();
    assert_eq!(material.0.load(Ordering::SeqCst), 1);
    assert_eq!(state(&db).await.1, before.1 + 1);
    assert_eq!(state(&db).await.2, before.2 + 1);
    store.close().await;
}
#[tokio::test]
async fn disabled_application_rejects_existing_tokens_and_client_authentication_after_commit() {
    use darkhorse_adapters::tokens::material::{self, Purpose};
    use darkhorse_application::{registration::ClientAuthenticationStore, tokens::TokenStore};
    let (db, signer) = super::tokens::setup().await;
    let code = super::tokens::code(&db, [3; 32]).await;
    let tokens = db
        .store
        .redeem(
            super::tokens::input(&code),
            material::pair().unwrap(),
            super::tokens::ISSUER,
            &signer,
        )
        .await
        .unwrap();
    let digest = material::digest(&tokens.access, Purpose::Access).unwrap();
    assert!(
        db.store
            .userinfo(digest, super::tokens::ISSUER)
            .await
            .is_ok()
    );
    write(
        &db.store.operator_applications(Material::default()),
        "one@example.com",
        update(0, 1, false),
    )
    .await
    .unwrap();
    assert!(
        db.store
            .userinfo(digest, super::tokens::ISSUER)
            .await
            .is_err()
    );
    assert!(
        db.store
            .authenticate_client(oidc::request().client, [9; 32])
            .await
            .is_err()
    );
}
#[tokio::test]
async fn application_audit_migration_preserves_historical_configuration_and_ledgers() {
    let db = Database::at_version(27).await;
    insert_principal(&db, 1, true).await;
    sqlx::raw_sql("INSERT INTO applications(id,name,owner_id,active) VALUES('00000000-0000-0000-0000-000000000010','Before upgrade','00000000-0000-0000-0000-000000000001',true); INSERT INTO operator_catalog_detail_audit(operation_id,command,application_id,result,occurred_ms) VALUES('00000000-0000-0000-0000-000000000002','application.show','00000000-0000-0000-0000-000000000010','denied',1);").execute(&db.pool).await.unwrap();
    let before: String =
        sqlx::query_scalar("SELECT row_to_json(a)::text FROM operator_catalog_detail_audit a")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    db.store
        .migrate_operation(OperationId::from_u128(3).unwrap())
        .await
        .unwrap();
    let after: String =
        sqlx::query_scalar("SELECT row_to_json(a)::text FROM operator_catalog_detail_audit a")
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
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM operator_application_audit")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    assert!(
        sqlx::query("UPDATE operator_catalog_detail_audit SET result='denied'")
            .execute(&db.pool)
            .await
            .is_err()
    );
    db.pool.close().await;
}

struct UnavailableEntropy;
impl Entropy for UnavailableEntropy {
    fn identifier(&self) -> Result<NonZeroU128, RegistrationError> {
        Err(RegistrationError::Unavailable)
    }
    fn secret(&self) -> Result<NewSecret, RegistrationError> {
        panic!("no client secrets")
    }
}
#[tokio::test]
async fn unavailable_entropy_and_identifier_collision_never_create_partial_or_duplicate_applications()
 {
    let db = oidc::fixture().await;
    let before = state(&db).await;
    assert!(matches!(
        write(
            &db.store.operator_applications(UnavailableEntropy),
            "one@example.com",
            Operation::Create(spec(1, true))
        )
        .await,
        Err(Error::Unavailable)
    ));
    assert_eq!(state(&db).await, before);
    let store = db.store.operator_applications(Material::default());
    write(&store, "one@example.com", Operation::Create(spec(1, true)))
        .await
        .unwrap();
    let before = state(&db).await;
    assert!(matches!(
        write(&store, "one@example.com", Operation::Create(spec(1, true))).await,
        Err(Error::Conflict)
    ));
    let after = state(&db).await;
    assert_eq!(
        (&before.0, before.1, before.3),
        (&after.0, after.1, after.3)
    );
    assert_eq!(after.2, before.2 + 1);
}
#[tokio::test]
async fn authority_loss_during_operator_audit_rolls_back_both_audits_and_the_application() {
    let db = oidc::fixture().await;
    let before = state(&db).await;
    sqlx::raw_sql("CREATE FUNCTION revoke_during_application_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE credentials SET revoked=true WHERE principal_id='00000000-0000-0000-0000-000000000001'; RETURN NEW; END $$; CREATE TRIGGER revoke_during_application_audit AFTER INSERT ON operator_application_audit FOR EACH ROW EXECUTE FUNCTION revoke_during_application_audit();").execute(&db.pool).await.unwrap();
    assert!(matches!(
        write(
            &db.store.operator_applications(Material::default()),
            "one@example.com",
            Operation::Create(spec(1, true))
        )
        .await,
        Err(Error::Denied)
    ));
    assert_eq!(state(&db).await, before);
}
