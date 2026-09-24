use super::*;
use darkhorse_application::{
    authentication::{AuthError, LoginAdmission, PasswordVerification},
    operator_accounts::{self, CandidateAt, Outcome, Store, Verified},
};
use darkhorse_domain::{
    identity::OperationId,
    operator_accounts::{Error, Operation, Request},
};
struct Allow;
impl LoginAdmission for Allow {
    async fn admit(&self, _: &str) -> Result<(), AuthError> {
        Ok(())
    }
}
struct Password(bool);
impl PasswordVerification for Password {
    async fn verify(&self, _: &str, _: Option<&str>) -> Result<bool, AuthError> {
        Ok(self.0)
    }
}
fn operation_id() -> OperationId {
    OperationId::from_u128(Uuid::new_v4().as_u128()).unwrap()
}
fn request(operation: Operation) -> Request {
    Request::new(operation, Some("Source-defined integration fixture")).unwrap()
}
fn change(action: AccountAction, revision: u64) -> Operation {
    Operation::Change {
        target: id(2),
        revision,
        action,
    }
}
async fn execute(
    store: &impl Store<Request = Request, Outcome = Outcome>,
    email: &str,
    operation: Operation,
) -> Result<Outcome, Error> {
    operator_accounts::run(
        store,
        &Allow,
        &Password(true),
        operation_id(),
        request(operation),
        email,
        "fixture password",
    )
    .await
}
async fn fixture() -> Database {
    let db = Database::new().await;
    db.store
        .bootstrap(administrator(1, "admin@example.com"))
        .await
        .unwrap();
    insert_principal(&db, 2, false).await;
    insert_password(&db, 2).await;
    db
}
#[tokio::test]
async fn operator_account_commands_record_verified_actor_and_revisions_without_browser_sessions() {
    let db = fixture().await;
    assert_eq!(
        execute(&db.store, "admin@example.com", Operation::Show(id(2)))
            .await
            .unwrap()
            .account
            .id,
        id(2)
    );
    for (operation, revision, epoch, changed) in [
        (
            change(AccountAction::SetStatus(AccountStatus::Inactive), 0),
            1,
            1,
            true,
        ),
        (
            change(AccountAction::SetStatus(AccountStatus::Inactive), 1),
            1,
            1,
            false,
        ),
        (
            change(AccountAction::SetStatus(AccountStatus::Active), 1),
            2,
            1,
            true,
        ),
        (change(AccountAction::RevokeAll, 2), 3, 2, true),
    ] {
        let result = execute(&db.store, "admin@example.com", operation)
            .await
            .unwrap();
        assert_eq!(
            (
                result.account.revision,
                result.account.credential_epoch,
                result.changed
            ),
            (revision, epoch, changed)
        );
    }
    let audit:(i64,i64,i64)=sqlx::query_as("SELECT count(*),count(*) FILTER(WHERE actor_id=$1 AND actor_credential_id=$2),count(*) FILTER(WHERE result='unchanged') FROM operator_account_audit")
        .bind(Uuid::from_u128(1)).bind(Uuid::from_u128(101)).fetch_one(&db.pool).await.unwrap();
    assert_eq!(audit, (5, 5, 1));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM browser_sessions")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
}
#[tokio::test]
async fn operator_denies_nonadministrators_and_wrong_passwords_and_retains_directory_invariants() {
    let db = fixture().await;
    assert!(matches!(
        execute(&db.store, "person2@example.com", Operation::Show(id(1))).await,
        Err(Error::Denied)
    ));
    assert!(matches!(
        operator_accounts::run(
            &db.store,
            &Allow,
            &Password(false),
            operation_id(),
            request(Operation::Show(id(2))),
            "admin@example.com",
            "wrong password"
        )
        .await,
        Err(Error::Denied)
    ));
    assert!(matches!(
        execute(&db.store, "missing@example.com", Operation::Show(id(1))).await,
        Err(Error::Denied)
    ));
    assert!(matches!(
        execute(&db.store, "admin@example.com", Operation::Show(id(999))).await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        execute(
            &db.store,
            "admin@example.com",
            change(AccountAction::RevokeAll, 99)
        )
        .await,
        Err(Error::Conflict)
    ));
    assert!(matches!(
        execute(
            &db.store,
            "admin@example.com",
            Operation::Change {
                target: id(1),
                revision: 0,
                action: AccountAction::SetStatus(AccountStatus::Inactive)
            }
        )
        .await,
        Err(Error::PolicyRejected)
    ));
    assert_eq!(db.store.account(id(2)).await.unwrap().credential_epoch, 0);
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT count(*),count(*) FILTER(WHERE actor_id IS NULL) FROM operator_account_audit",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(counts, (6, 2));
}
struct Aged<'a>(&'a PostgresStore);
impl Store for Aged<'_> {
    type Request = Request;
    type Outcome = Outcome;
    async fn candidate(&self, email: &str) -> Result<Option<CandidateAt>, Error> {
        let mut candidate = Store::candidate(self.0, email).await?;
        if let Some(c) = &mut candidate {
            c.observed_ms -= 60_000;
        }
        Ok(candidate)
    }
    async fn denied(&self, id: OperationId, request: &Request) -> Result<(), Error> {
        self.0.denied(id, request).await
    }
    async fn execute(&self, proof: Verified) -> Result<Outcome, Error> {
        self.0.execute(proof).await
    }
}
#[tokio::test]
async fn expired_proof_and_failed_audit_cannot_read_or_change_an_account() {
    let db = fixture().await;
    assert!(matches!(
        execute(
            &Aged(&db.store),
            "admin@example.com",
            change(AccountAction::RevokeAll, 0)
        )
        .await,
        Err(Error::Denied)
    ));
    sqlx::raw_sql("CREATE FUNCTION reject_operator_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture audit failure'; END $$; CREATE TRIGGER reject_operator_audit BEFORE INSERT ON operator_account_audit FOR EACH ROW EXECUTE FUNCTION reject_operator_audit();").execute(&db.pool).await.unwrap();
    for operation in [Operation::Show(id(2)), change(AccountAction::RevokeAll, 0)] {
        assert!(matches!(
            execute(&db.store, "admin@example.com", operation).await,
            Err(Error::Unavailable)
        ));
    }
    assert_eq!(db.store.account(id(2)).await.unwrap().credential_epoch, 0);
    assert_eq!(count(&db, "security_audit").await, 1);
}

struct Paused<'a> {
    inner: &'a PostgresStore,
    ready: tokio::sync::Notify,
    resume: tokio::sync::Notify,
}
impl Store for Paused<'_> {
    type Request = Request;
    type Outcome = Outcome;
    async fn candidate(&self, email: &str) -> Result<Option<CandidateAt>, Error> {
        let candidate = Store::candidate(self.inner, email).await?;
        self.ready.notify_one();
        self.resume.notified().await;
        Ok(candidate)
    }
    async fn denied(&self, id: OperationId, request: &Request) -> Result<(), Error> {
        self.inner.denied(id, request).await
    }
    async fn execute(&self, proof: Verified) -> Result<Outcome, Error> {
        self.inner.execute(proof).await
    }
}
#[tokio::test]
async fn committed_actor_reductions_invalidate_a_password_proof_captured_before_the_change() {
    for sql in [
        "DELETE FROM platform_administrators WHERE principal_id=$1",
        "UPDATE principals SET active=false,revision=revision+1,credential_epoch=credential_epoch+1 WHERE id=$1",
        "UPDATE principals SET revision=revision+1,credential_epoch=credential_epoch+1 WHERE id=$1",
        "UPDATE credentials SET revoked=true WHERE principal_id=$1",
        "UPDATE password_credentials SET verifier=verifier||'changed' WHERE credential_id IN(SELECT id FROM credentials WHERE principal_id=$1)",
    ] {
        let db = fixture().await;
        insert_principal(&db, 3, true).await;
        let paused = Paused {
            inner: &db.store,
            ready: tokio::sync::Notify::new(),
            resume: tokio::sync::Notify::new(),
        };
        let operation = execute(
            &paused,
            "admin@example.com",
            change(AccountAction::RevokeAll, 0),
        );
        let reduce = async {
            paused.ready.notified().await;
            let mut tx = db.pool.begin().await.unwrap();
            sqlx::query("SELECT singleton FROM security_state FOR UPDATE")
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(Uuid::from_u128(1))
                .execute(&mut *tx)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            paused.resume.notify_one();
        };
        let (result, ()) = tokio::join!(operation, reduce);
        assert!(matches!(result, Err(Error::Denied)), "{sql}");
        assert_eq!(db.store.account(id(2)).await.unwrap().credential_epoch, 0);
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT result FROM operator_account_audit")
                .fetch_one(&db.pool)
                .await
                .unwrap(),
            "denied"
        );
    }
}
#[tokio::test]
async fn a_failed_commit_is_reported_as_uncertain_without_retry() {
    let db = fixture().await;
    sqlx::raw_sql("CREATE FUNCTION reject_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture commit failure'; END $$; CREATE CONSTRAINT TRIGGER reject_commit AFTER INSERT ON operator_account_audit DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_commit();").execute(&db.pool).await.unwrap();
    assert!(matches!(
        execute(
            &db.store,
            "admin@example.com",
            change(AccountAction::RevokeAll, 0)
        )
        .await,
        Err(Error::Uncertain)
    ));
    assert_eq!(db.store.account(id(2)).await.unwrap().credential_epoch, 0);
    assert_eq!(count(&db, "security_audit").await, 1);
}

#[tokio::test]
async fn losing_the_commit_reply_returns_uncertain_with_one_committed_operation() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };
    let db = fixture().await;
    let original = db.pool.connect_options();
    let upstream = (original.get_host().to_owned(), original.get_port());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let proxy = tokio::spawn(async move {
        let (client, _) = listener.accept().await.unwrap();
        let server = TcpStream::connect(upstream).await.unwrap();
        let (mut read_client, mut write_client) = client.into_split();
        let (mut read_server, mut write_server) = server.into_split();
        let forward = tokio::spawn(async move {
            let _ = tokio::io::copy(&mut read_client, &mut write_server).await;
        });
        loop {
            let mut header = [0; 5];
            read_server.read_exact(&mut header).await.unwrap();
            let size = u32::from_be_bytes(header[1..].try_into().unwrap()) as usize;
            assert!((4..=1_048_576).contains(&size));
            let mut payload = vec![0; size - 4];
            read_server.read_exact(&mut payload).await.unwrap();
            if header[0] == b'C' && payload == b"COMMIT\0" {
                // PostgreSQL has committed; discard its acknowledgement and close
                // the client connection rather than simulating a rolled-back error.
                forward.abort();
                write_client.shutdown().await.unwrap();
                break;
            }
            write_client.write_all(&header).await.unwrap();
            write_client.write_all(&payload).await.unwrap();
        }
    });
    let options = (*original)
        .clone()
        .host("127.0.0.1")
        .port(port)
        .ssl_mode(sqlx::postgres::PgSslMode::Disable);
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    let store = PostgresStore::from_pool(pool);
    assert!(matches!(
        execute(
            &store,
            "admin@example.com",
            change(AccountAction::RevokeAll, 0)
        )
        .await,
        Err(Error::Uncertain)
    ));
    proxy.await.unwrap();
    assert_eq!(db.store.account(id(2)).await.unwrap().credential_epoch, 1);
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

#[tokio::test]
async fn an_operator_waiting_at_the_security_fence_observes_committed_demotion() {
    let db = fixture().await;
    insert_principal(&db, 3, true).await;
    let paused = Paused {
        inner: &db.store,
        ready: tokio::sync::Notify::new(),
        resume: tokio::sync::Notify::new(),
    };
    let mut tx = db.pool.begin().await.unwrap();
    sqlx::query("SELECT singleton FROM security_state FOR UPDATE")
        .execute(&mut *tx)
        .await
        .unwrap();
    let operation = execute(
        &paused,
        "admin@example.com",
        change(AccountAction::RevokeAll, 0),
    );
    let reduce = async {
        paused.ready.notified().await;
        paused.resume.notify_one();
        tokio::time::timeout(std::time::Duration::from_secs(5),async {
            loop {
                let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE '%security_state%')").fetch_one(&db.pool).await.unwrap();
                if blocked{break;}
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.expect("operator must wait on the held security fence");
        sqlx::query("DELETE FROM platform_administrators WHERE principal_id=$1")
            .bind(Uuid::from_u128(1))
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    };
    let (result, ()) = tokio::join!(operation, reduce);
    assert!(matches!(result, Err(Error::Denied)));
    assert_eq!(db.store.account(id(2)).await.unwrap().credential_epoch, 0);
    assert!(matches!(
        execute(&db.store, "admin@example.com", Operation::Show(id(2))).await,
        Err(Error::Denied)
    ));
}

#[tokio::test]
async fn suppressed_account_audit_rolls_back_mutation() {
    let db = fixture().await;
    sqlx::raw_sql("CREATE FUNCTION suppress_account_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NULL; END $$; CREATE TRIGGER suppress_account_audit BEFORE INSERT ON operator_account_audit FOR EACH ROW EXECUTE FUNCTION suppress_account_audit();").execute(&db.pool).await.unwrap();
    assert!(matches!(
        execute(
            &db.store,
            "admin@example.com",
            change(AccountAction::RevokeAll, 0)
        )
        .await,
        Err(Error::Unavailable)
    ));
    assert_eq!(db.store.account(id(2)).await.unwrap().credential_epoch, 0);
}
