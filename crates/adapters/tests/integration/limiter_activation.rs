use super::*;
use darkhorse_application::{
    limiter_activation::{Error, Journal},
    shared_limiting::{EnforcementAuthority, RecoveryAuthority},
};
use darkhorse_domain::{identity::OperationId, limiter_recovery::ServerIdentity};
fn operation(n: u128) -> OperationId {
    OperationId::from_u128(n).unwrap()
}
fn server() -> ServerIdentity {
    ServerIdentity {
        run: [1; 20],
        replication: [2; 20],
    }
}
async fn ready(db: &Database) {
    db.store.fence([1; 16]).await.unwrap();
    sqlx::raw_sql("ALTER TABLE limiter_authority DISABLE TRIGGER limiter_authority_transition; UPDATE limiter_authority SET not_before_ms=0; ALTER TABLE limiter_authority ENABLE TRIGGER limiter_authority_transition;").execute(&db.pool).await.unwrap();
}
#[tokio::test]
async fn intent_is_durable_and_cannot_skip_wait_or_reuse_an_operation() {
    let db = Database::new().await;
    assert_eq!(db.store.inspect(operation(1)).await, Err(Error::NotFound));
    db.store.fence([1; 16]).await.unwrap();
    assert_eq!(db.store.prepare(operation(1)).await, Err(Error::NotReady));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM limiter_activation_intents")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    sqlx::raw_sql("ALTER TABLE limiter_authority DISABLE TRIGGER limiter_authority_transition; UPDATE limiter_authority SET not_before_ms=0; ALTER TABLE limiter_authority ENABLE TRIGGER limiter_authority_transition;").execute(&db.pool).await.unwrap();
    let state = db.store.prepare(operation(1)).await.unwrap();
    let record = db.store.inspect(operation(1)).await.unwrap();
    assert_eq!(record.generation, state.generation);
    assert!(record.prepared_ms >= record.not_before_ms);
    assert!(record.completion.is_none());
    assert!(!record.current.active);
    let role: String = sqlx::query_scalar("SELECT session_user::text")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(record.database_role, role);
    assert_eq!(
        db.store.prepare(operation(1)).await,
        Err(Error::Unavailable)
    );
    assert_eq!(
        db.store
            .complete(operation(2), state.generation, server())
            .await,
        Err(Error::Unavailable)
    );
    db.store.close().await;
}
#[tokio::test]
async fn completion_and_activation_are_atomic_and_inspection_is_historical() {
    let db = Database::new().await;
    ready(&db).await;
    let state = db.store.prepare(operation(1)).await.unwrap();
    db.store
        .complete(operation(1), state.generation, server())
        .await
        .unwrap();
    let record = db.store.inspect(operation(1)).await.unwrap();
    assert!(record.current.active);
    assert_eq!(record.completion.as_ref().unwrap().identity, server());
    assert!(record.completion.as_ref().unwrap().completed_ms >= record.prepared_ms);
    assert_eq!(
        db.store
            .complete(operation(1), state.generation, server())
            .await,
        Err(Error::Unavailable)
    );
    db.store.fence([2; 16]).await.unwrap();
    let record = db.store.inspect(operation(1)).await.unwrap();
    assert!(!record.current.active);
    assert_ne!(record.current.generation, record.generation);
    assert!(record.completion.is_some());
    for statement in [
        "UPDATE limiter_activation_intents SET epoch=epoch",
        "DELETE FROM limiter_activation_intents",
        "UPDATE limiter_activation_receipts SET completed_ms=completed_ms",
        "DELETE FROM limiter_activation_receipts",
    ] {
        assert!(sqlx::raw_sql(statement).execute(&db.pool).await.is_err());
    }
    db.store.close().await;
}
#[tokio::test]
async fn receipt_or_legacy_audit_failure_keeps_the_authority_inactive_and_intent_pending() {
    for table in ["limiter_activation_receipts", "limiter_audit"] {
        let db = Database::new().await;
        ready(&db).await;
        let state = db.store.prepare(operation(1)).await.unwrap();
        let statement = format!(
            "CREATE FUNCTION reject_activation() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected'; END $$; CREATE TRIGGER reject_activation BEFORE INSERT ON {table} FOR EACH ROW EXECUTE FUNCTION reject_activation();"
        );
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement))
            .execute(&db.pool)
            .await
            .unwrap();
        assert_eq!(
            db.store
                .complete(operation(1), state.generation, server())
                .await,
            Err(Error::Unavailable)
        );
        let record = db.store.inspect(operation(1)).await.unwrap();
        assert!(!record.current.active);
        assert!(record.completion.is_none());
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM limiter_audit WHERE event='limiter.activated'"
            )
            .fetch_one(&db.pool)
            .await
            .unwrap(),
            0
        );
        db.store.close().await;
    }
}
#[tokio::test]
async fn competing_activations_have_one_receipt_and_new_fences_reject_old_intents() {
    let db = Database::new().await;
    ready(&db).await;
    let first = db.store.prepare(operation(1)).await.unwrap();
    let second = db.store.prepare(operation(2)).await.unwrap();
    let (a, b) = tokio::join!(
        db.store.complete(operation(1), first.generation, server()),
        db.store.complete(operation(2), second.generation, server())
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM limiter_activation_receipts")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    db.store.fence([2; 16]).await.unwrap();
    assert_eq!(
        db.store
            .complete(
                operation(if a.is_ok() { 2 } else { 1 }),
                first.generation,
                server()
            )
            .await,
        Err(Error::Unavailable)
    );
    assert!(!db.store.read().await.unwrap().active);
    db.store.close().await;
}
#[tokio::test]
async fn schema_upgrade_requires_new_journal_without_inventing_historical_receipts() {
    let db = Database::at_version(22).await;
    ready(&db).await;
    assert_eq!(
        db.store.prepare(operation(1)).await,
        Err(Error::Unavailable)
    );
    assert!(!db.store.read().await.unwrap().active);
    db.store.migrate().await.unwrap();
    let state = db.store.prepare(operation(1)).await.unwrap();
    db.store
        .complete(operation(1), state.generation, server())
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM limiter_activation_intents")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    db.store.close().await;
}
#[tokio::test]
async fn failed_completion_commit_is_uncertain_and_leaves_the_committed_intent() {
    let db = Database::new().await;
    ready(&db).await;
    let state = db.store.prepare(operation(1)).await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION reject_activation_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected commit failure'; END $$; CREATE CONSTRAINT TRIGGER reject_activation_commit AFTER INSERT ON limiter_activation_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_activation_commit();").execute(&db.pool).await.unwrap();
    assert_eq!(
        db.store
            .complete(operation(1), state.generation, server())
            .await,
        Err(Error::Uncertain)
    );
    let record = db.store.inspect(operation(1)).await.unwrap();
    assert!(!record.current.active);
    assert!(record.completion.is_none());
    db.store.close().await;
}
#[tokio::test]
async fn actual_lost_commit_responses_preserve_inspectable_intent_or_completed_result() {
    for completion in [false, true] {
        let db = Database::new().await;
        ready(&db).await;
        let state = if completion {
            Some(db.store.prepare(operation(1)).await.unwrap())
        } else {
            None
        };
        let (store, proxy) = lost_commit(&db.pool).await;
        let result = if let Some(state) = state {
            store
                .complete(operation(1), state.generation, server())
                .await
        } else {
            store.prepare(operation(1)).await.map(|_| ())
        };
        assert_eq!(result, Err(Error::Uncertain));
        proxy.await.unwrap();
        let record = db.store.inspect(operation(1)).await.unwrap();
        assert_eq!(record.completion.is_some(), completion);
        assert_eq!(record.current.active, completion);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM limiter_activation_intents")
                .fetch_one(&db.pool)
                .await
                .unwrap(),
            1
        );
        store.close().await;
        db.store.close().await;
    }
}
pub(super) async fn lost_commit(pool: &PgPool) -> (PostgresStore, tokio::task::JoinHandle<()>) {
    use tokio::{
        io::AsyncWriteExt,
        net::{TcpListener, TcpStream},
    };
    let original = pool.connect_options();
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
            let (header, payload) = message(&mut read_server).await;
            if header[0] == b'C' && payload == b"COMMIT\0" {
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
    (PostgresStore::from_pool(pool), proxy)
}
async fn message(reader: &mut tokio::net::tcp::OwnedReadHalf) -> ([u8; 5], Vec<u8>) {
    use tokio::io::AsyncReadExt;
    let mut header = [0; 5];
    reader.read_exact(&mut header).await.unwrap();
    let size = u32::from_be_bytes(header[1..].try_into().unwrap()) as usize;
    assert!((4..=1_048_576).contains(&size));
    let mut payload = vec![0; size - 4];
    reader.read_exact(&mut payload).await.unwrap();
    (header, payload)
}
