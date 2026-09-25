use super::operator_directory::{Allow, Password};
use super::*;
use darkhorse_application::operator_accounts::{self, CandidateAt, Store, Verified};
use darkhorse_domain::{
    identity::{ApplicationId, ClientId, OperationId},
    operator_accounts::{Error, Operation, Request},
    operator_catalog::{Definitions, Target},
};

async fn run<S: Store>(store: &S, request: S::Request) -> Result<S::Outcome, Error> {
    operator_accounts::run(
        store,
        &Allow,
        &Password(true),
        OperationId::from_u128(Uuid::new_v4().as_u128()).unwrap(),
        request,
        "one@example.com",
        "source-only passphrase",
    )
    .await
}
fn account(operation: Operation) -> Request {
    Request::new(operation, Some("Source-defined authority qualification")).unwrap()
}
fn change(target: u128, revision: u64, action: AccountAction) -> Operation {
    Operation::Change {
        target: id(target),
        revision,
        action,
    }
}
pub(super) const REDUCTIONS: [&str; 5] = [
    "DELETE FROM platform_administrators WHERE principal_id=NEW.actor_id;",
    "UPDATE credentials SET revoked=true WHERE id=NEW.actor_credential_id;",
    "UPDATE password_credentials SET verifier=verifier||'changed' WHERE credential_id=NEW.actor_credential_id;",
    "UPDATE principals SET credential_epoch=credential_epoch+1,revision=revision+1 WHERE id=NEW.actor_id;",
    "UPDATE principals SET active=false,credential_epoch=credential_epoch+1,revision=revision+1 WHERE id=NEW.actor_id;",
];
pub(super) async fn inject(db: &Database, table: &str, body: &str) {
    // Identifiers and SQL are source-defined fixture values only.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "CREATE FUNCTION authority_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN {body} RETURN NEW; END $$; CREATE TRIGGER authority_fault AFTER INSERT ON {table} FOR EACH ROW EXECUTE FUNCTION authority_fault();"
    ))).execute(&db.pool).await.unwrap();
}
pub(super) async fn remove(db: &Database, table: &str) {
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP TRIGGER authority_fault ON {table}; DROP FUNCTION authority_fault();"
    )))
    .execute(&db.pool)
    .await
    .unwrap();
}
pub(super) async fn untouched(db: &Database, table: &str) {
    assert_eq!(
        sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    assert!(sqlx::query_scalar::<_, bool>("SELECT p.active AND p.credential_epoch=0 AND p.revision=0 AND NOT c.revoked AND EXISTS(SELECT 1 FROM platform_administrators WHERE principal_id=p.id) FROM principals p JOIN credentials c ON c.principal_id=p.id WHERE p.id='00000000-0000-0000-0000-000000000001'").fetch_one(&db.pool).await.unwrap());
}

#[tokio::test]
async fn account_audit_rechecks_records_errors_and_expected_self_changes() {
    for reduction in REDUCTIONS {
        let db = oidc::fixture().await;
        insert_principal(&db, 2, false).await;
        insert_principal(&db, 3, true).await;
        inject(&db, "operator_account_audit", reduction).await;
        for operation in [
            Operation::Show(id(2)),
            Operation::Show(id(999)),
            change(2, 99, AccountAction::RevokeAll),
            change(2, 0, AccountAction::SetStatus(AccountStatus::Active)),
            change(2, 0, AccountAction::RevokeAll),
            change(1, 0, AccountAction::RevokeAll),
            change(1, 0, AccountAction::SetStatus(AccountStatus::Inactive)),
        ] {
            assert!(
                matches!(run(&db.store, account(operation)).await, Err(Error::Denied)),
                "{operation:?}: {reduction}"
            );
            untouched(&db, "operator_account_audit").await;
            assert_eq!(db.store.account(id(2)).await.unwrap().credential_epoch, 0);
        }
    }
}

fn catalog(target: Target) -> darkhorse_domain::operator_catalog::Request {
    darkhorse_domain::operator_catalog::Request::new(
        target,
        darkhorse_domain::admin_catalog::Query {
            search: String::new(),
            active: None,
            after: None,
            limit: 25,
        },
    )
    .unwrap()
}
fn directory() -> darkhorse_domain::operator_directory::Request {
    darkhorse_domain::operator_directory::Request::new(darkhorse_domain::admin_directory::Query {
        search: String::new(),
        status: None,
        after: None,
        limit: 25,
    })
    .unwrap()
}
fn secrets(application: u128, client: u128) -> darkhorse_domain::operator_client_secrets::Request {
    use darkhorse_domain::operator_client_secrets::{Operation, Request, Target};
    Request::new(
        Target {
            application: ApplicationId::from_u128(application).unwrap(),
            client: ClientId::from_u128(client).unwrap(),
        },
        Operation::List {
            after: None,
            limit: 25,
        },
        None,
    )
    .unwrap()
}
#[tokio::test]
async fn list_audits_recheck_authority_for_pages_missing_and_cross_application_targets() {
    for reduction in REDUCTIONS {
        let db = oidc::fixture().await;
        insert_principal(&db, 3, true).await;
        inject(&db, "operator_directory_audit", reduction).await;
        assert!(matches!(
            run(&db.store.operator_directory(), directory()).await,
            Err(Error::Denied)
        ));
        untouched(&db, "operator_directory_audit").await;
        remove(&db, "operator_directory_audit").await;
        inject(&db, "operator_catalog_audit", reduction).await;
        let missing = ApplicationId::from_u128(999).unwrap();
        for target in [
            Target::Applications,
            Target::Clients(missing),
            Target::Resources(missing),
            Target::Scopes(missing),
            Target::Roles(Definitions::Application(missing)),
            Target::Capabilities(Definitions::Application(missing)),
        ] {
            assert!(
                matches!(
                    run(&db.store.operator_catalog(), catalog(target)).await,
                    Err(Error::Denied)
                ),
                "{target:?}"
            );
            untouched(&db, "operator_catalog_audit").await;
        }
        remove(&db, "operator_catalog_audit").await;
        inject(&db, "operator_client_secret_audit", reduction).await;
        for (application, client) in [(16, 32), (16, 999), (999, 32)] {
            assert!(matches!(
                run(
                    &db.store.operator_client_secrets(),
                    secrets(application, client)
                )
                .await,
                Err(Error::Denied)
            ));
            untouched(&db, "operator_client_secret_audit").await;
        }
    }
}

#[tokio::test]
async fn suppressed_account_write_or_security_audit_rolls_back_without_success() {
    for table in ["principals", "security_audit"] {
        let db = oidc::fixture().await;
        insert_principal(&db, 2, false).await;
        let event = if table == "principals" {
            "UPDATE"
        } else {
            "INSERT"
        };
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION suppress_effect() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NULL; END $$; CREATE TRIGGER suppress_effect BEFORE {event} ON {table} FOR EACH ROW EXECUTE FUNCTION suppress_effect();"))).execute(&db.pool).await.unwrap();
        assert!(
            matches!(
                run(&db.store, account(change(2, 0, AccountAction::RevokeAll))).await,
                Err(Error::Unavailable)
            ),
            "{table}"
        );
        assert_eq!(db.store.account(id(2)).await.unwrap().credential_epoch, 0);
        untouched(&db, "operator_account_audit").await;
    }
}

#[tokio::test]
async fn self_changes_commit_only_their_expected_transition_and_leave_one_eligible_admin() {
    let db = oidc::fixture().await;
    insert_principal(&db, 3, true).await;
    let revoked = run(&db.store, account(change(1, 0, AccountAction::RevokeAll)))
        .await
        .unwrap();
    assert_eq!(
        (
            revoked.account.credential_epoch,
            revoked.account.revision,
            revoked.changed
        ),
        (1, 1, true)
    );
    assert_eq!(revoked.account.status, AccountStatus::Active);
    let deactivated = run(
        &db.store,
        account(change(
            1,
            1,
            AccountAction::SetStatus(AccountStatus::Inactive),
        )),
    )
    .await
    .unwrap();
    assert_eq!(
        (
            deactivated.account.credential_epoch,
            deactivated.account.revision,
            deactivated.changed
        ),
        (2, 2, true)
    );
    assert_eq!(deactivated.account.status, AccountStatus::Inactive);
    assert!(matches!(
        run(&db.store, account(Operation::Show(id(3)))).await,
        Err(Error::Denied)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM eligible_administrators")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM operator_account_audit WHERE result='changed' AND actor_id=target_id AND target_revision=actor_epoch+1").fetch_one(&db.pool).await.unwrap(), 2);
}

#[tokio::test]
async fn concurrent_self_deactivations_preserve_the_last_administrator() {
    let db = oidc::fixture().await;
    insert_principal(&db, 3, true).await;
    let first = run(
        &db.store,
        account(change(
            1,
            0,
            AccountAction::SetStatus(AccountStatus::Inactive),
        )),
    );
    let second = operator_accounts::run(
        &db.store,
        &Allow,
        &Password(true),
        OperationId::from_u128(300).unwrap(),
        account(change(
            3,
            0,
            AccountAction::SetStatus(AccountStatus::Inactive),
        )),
        "person3@example.com",
        "fixture",
    );
    let (a, b) = tokio::join!(first, second);
    assert!(matches!(
        (&a, &b),
        (Ok(_), Err(Error::PolicyRejected)) | (Err(Error::PolicyRejected), Ok(_))
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM eligible_administrators")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(sqlx::query_as::<_, (i64,i64)>("SELECT count(*) FILTER(WHERE result='changed'),count(*) FILTER(WHERE result='policy_rejected') FROM operator_account_audit").fetch_one(&db.pool).await.unwrap(), (1,1));
}

#[tokio::test]
async fn policy_rejection_needs_authority_but_unrelated_reduction_does_not_deny_the_actor() {
    let db = oidc::fixture().await;
    inject(&db, "operator_account_audit", "UPDATE principals SET credential_epoch=credential_epoch+1,revision=revision+1 WHERE id=NEW.actor_id;").await;
    assert!(matches!(
        run(
            &db.store,
            account(change(
                1,
                0,
                AccountAction::SetStatus(AccountStatus::Inactive)
            ))
        )
        .await,
        Err(Error::Denied)
    ));
    untouched(&db, "operator_account_audit").await;
    remove(&db, "operator_account_audit").await;
    insert_principal(&db, 3, true).await;
    inject(&db, "operator_account_audit", "DELETE FROM platform_administrators WHERE principal_id='00000000-0000-0000-0000-000000000003';").await;
    assert!(
        run(&db.store, account(Operation::Show(id(1))))
            .await
            .is_ok()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM platform_administrators")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
}

pub(super) struct Aged<S>(pub(super) S);
impl<S: Store> Store for Aged<S> {
    type Request = S::Request;
    type Outcome = S::Outcome;
    async fn candidate(&self, email: &str) -> Result<Option<CandidateAt>, Error> {
        let mut candidate = self.0.candidate(email).await?;
        if let Some(candidate) = &mut candidate {
            candidate.observed_ms -= 58_000;
        }
        Ok(candidate)
    }
    async fn denied(&self, id: OperationId, request: &Self::Request) -> Result<(), Error> {
        self.0.denied(id, request).await
    }
    async fn execute(&self, proof: Verified<Self::Request>) -> Result<Self::Outcome, Error> {
        self.0.execute(proof).await
    }
}
pub(super) async fn expiry(db: &Database, table: &str) {
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE OR REPLACE FUNCTION expire_authority() RETURNS trigger LANGUAGE plpgsql AS $$ DECLARE remaining double precision; BEGIN remaining := (NEW.authentication_observed_ms+60000)/1000.0-extract(epoch FROM clock_timestamp()); IF NEW.result='denied' OR remaining<=0 THEN RAISE EXCEPTION 'fixture must reach audit with live proof'; END IF; PERFORM nextval('authority_audit_reached'); PERFORM pg_sleep(remaining+0.025); RETURN NEW; END $$; CREATE TRIGGER expire_authority BEFORE INSERT ON {table} FOR EACH ROW EXECUTE FUNCTION expire_authority();"))).execute(&db.pool).await.unwrap();
}
#[tokio::test]
async fn account_audit_expiry_rolls_back_reads_errors_noops_and_self_changes() {
    let db = oidc::fixture().await;
    sqlx::query("CREATE SEQUENCE authority_audit_reached")
        .execute(&db.pool)
        .await
        .unwrap();
    expiry(&db, "operator_account_audit").await;
    let aged = Aged(db.store.clone());
    for operation in [
        Operation::Show(id(1)),
        Operation::Show(id(999)),
        change(1, 99, AccountAction::RevokeAll),
        change(1, 0, AccountAction::SetStatus(AccountStatus::Inactive)),
        change(1, 0, AccountAction::SetStatus(AccountStatus::Active)),
        change(1, 0, AccountAction::RevokeAll),
    ] {
        assert!(
            matches!(run(&aged, account(operation)).await, Err(Error::Denied)),
            "{operation:?}"
        );
        untouched(&db, "operator_account_audit").await;
    }
    insert_principal(&db, 3, true).await;
    assert!(matches!(
        run(
            &aged,
            account(change(
                1,
                0,
                AccountAction::SetStatus(AccountStatus::Inactive)
            ))
        )
        .await,
        Err(Error::Denied)
    ));
    untouched(&db, "operator_account_audit").await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT last_value FROM authority_audit_reached")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        7
    );
}
#[tokio::test]
async fn list_audit_expiry_releases_no_page_or_state_dependent_error() {
    let db = oidc::fixture().await;
    sqlx::query("CREATE SEQUENCE authority_audit_reached")
        .execute(&db.pool)
        .await
        .unwrap();
    for table in [
        "operator_directory_audit",
        "operator_catalog_audit",
        "operator_client_secret_audit",
    ] {
        expiry(&db, table).await;
    }
    assert!(matches!(
        run(&Aged(db.store.operator_directory()), directory()).await,
        Err(Error::Denied)
    ));
    for target in [
        Target::Applications,
        Target::Clients(ApplicationId::from_u128(999).unwrap()),
    ] {
        assert!(matches!(
            run(&Aged(db.store.operator_catalog()), catalog(target)).await,
            Err(Error::Denied)
        ));
    }
    for (application, client) in [(16, 32), (16, 999), (999, 32)] {
        assert!(matches!(
            run(
                &Aged(db.store.operator_client_secrets()),
                secrets(application, client)
            )
            .await,
            Err(Error::Denied)
        ));
    }
    for table in [
        "operator_directory_audit",
        "operator_catalog_audit",
        "operator_client_secret_audit",
    ] {
        untouched(&db, table).await;
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT last_value FROM authority_audit_reached")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        6
    );
}

#[tokio::test]
async fn proof_expiring_at_target_lock_is_rechecked_before_any_account_write() {
    let db = oidc::fixture().await;
    insert_principal(&db, 2, false).await;
    sqlx::raw_sql("CREATE SEQUENCE account_write_reached; CREATE FUNCTION mark_account_write() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('account_write_reached'); RETURN NEW; END $$; CREATE TRIGGER mark_account_write BEFORE UPDATE ON principals FOR EACH ROW EXECUTE FUNCTION mark_account_write();").execute(&db.pool).await.unwrap();
    let mut blocker = db.pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM principals WHERE id=$1 FOR UPDATE")
        .bind(Uuid::from_u128(2))
        .execute(&mut *blocker)
        .await
        .unwrap();
    let held = super::operator_directory::Held {
        inner: db.store.clone(),
        ready: tokio::sync::Notify::new(),
        resume: tokio::sync::Notify::new(),
        age: 58_000,
    };
    let release = async {
        held.ready.notified().await;
        held.resume.notify_one();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE '%FOR UPDATE OF p%')").fetch_one(&db.pool).await.unwrap();
                if waiting { break; }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(2050)).await;
        blocker.commit().await.unwrap();
    };
    let (result, ()) = tokio::join!(
        run(&held, account(change(2, 0, AccountAction::RevokeAll))),
        release
    );
    assert!(matches!(result, Err(Error::Denied)));
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT is_called FROM account_write_reached")
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
    assert_eq!(db.store.account(id(2)).await.unwrap().credential_epoch, 0);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM operator_account_audit WHERE result='denied'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        1
    );
}

#[tokio::test]
async fn lost_self_revocation_commit_reply_has_one_committed_transition_and_no_retry() {
    let db = oidc::fixture().await;
    let (store, proxy) = super::limiter_activation::lost_nth_commit(&db.pool, 1).await;
    assert!(matches!(
        run(&store, account(change(1, 0, AccountAction::RevokeAll))).await,
        Err(Error::Uncertain)
    ));
    proxy.await.unwrap();
    let current = db.store.account(id(1)).await.unwrap();
    assert_eq!((current.credential_epoch, current.revision), (1, 1));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM operator_account_audit WHERE result='changed'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        1
    );
    store.close().await;
}

async fn waiting_reduction<S: Store>(
    db: &Database,
    inner: S,
    request: S::Request,
    reduction: &str,
) {
    let held = super::operator_directory::Held {
        inner,
        ready: tokio::sync::Notify::new(),
        resume: tokio::sync::Notify::new(),
        age: 0,
    };
    let mut writer = db.pool.begin().await.unwrap();
    sqlx::query("SELECT singleton FROM security_state FOR UPDATE")
        .execute(&mut *writer)
        .await
        .unwrap();
    let reduce = async {
        held.ready.notified().await;
        held.resume.notify_one();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE '%security_state%')").fetch_one(&db.pool).await.unwrap();
                if waiting { break; }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.unwrap();
        // Reuse source-defined faults with the fixture's known actor and credential.
        let sql = reduction
            .replace(
                "NEW.actor_credential_id",
                "'00000000-0000-0000-0000-000000000065'::uuid",
            )
            .replace(
                "NEW.actor_id",
                "'00000000-0000-0000-0000-000000000001'::uuid",
            );
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
            .execute(&mut *writer)
            .await
            .unwrap();
        writer.commit().await.unwrap();
    };
    let (result, ()) = tokio::join!(run(&held, request), reduce);
    assert!(matches!(result, Err(Error::Denied)), "{reduction}");
}
#[tokio::test]
async fn shared_and_exclusive_waiters_observe_every_committed_actor_reduction() {
    for (index, reduction) in REDUCTIONS.into_iter().enumerate() {
        for kind in 0..5 {
            let db = oidc::fixture().await;
            insert_principal(&db, 3, true).await;
            match kind {
                0 => {
                    waiting_reduction(
                        &db,
                        db.store.clone(),
                        account(Operation::Show(id(999))),
                        reduction,
                    )
                    .await
                }
                1 => {
                    waiting_reduction(
                        &db,
                        db.store.clone(),
                        account(change(1, 0, AccountAction::RevokeAll)),
                        reduction,
                    )
                    .await
                }
                2 => {
                    waiting_reduction(&db, db.store.operator_directory(), directory(), reduction)
                        .await
                }
                3 => {
                    waiting_reduction(
                        &db,
                        db.store.operator_catalog(),
                        catalog(Target::Clients(ApplicationId::from_u128(999).unwrap())),
                        reduction,
                    )
                    .await
                }
                _ => {
                    waiting_reduction(
                        &db,
                        db.store.operator_client_secrets(),
                        secrets(999, 32),
                        reduction,
                    )
                    .await
                }
            }
            let fresh = run(&db.store, account(Operation::Show(id(1)))).await;
            if matches!(index, 2 | 3) {
                // The password fake accepts the current verifier. A new valid password
                // proof may authenticate after verifier replacement or an epoch advance.
                assert!(fresh.is_ok());
            } else {
                assert!(matches!(fresh, Err(Error::Denied)));
            }
        }
    }
}
