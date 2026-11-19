use sqlx::{Connection, PgConnection};
use std::time::Duration;

// Created only after this harness successfully creates its random database.
// Dropped after the fixture's pools; an independent runtime also works while
// the test runtime is unwinding. Never select databases by age or a broad prefix.
pub(super) struct DatabaseCleanup {
    pub(super) name: String,
    pub(super) admin_url: String,
}

impl Drop for DatabaseCleanup {
    fn drop(&mut self) {
        let name = self.name.clone();
        let admin_url = self.admin_url.clone();
        let result = std::thread::Builder::new()
            .name("database-fixture-cleanup".into())
            .spawn(move || remove_database(&admin_url, &name))
            .map_err(|_| ())
            .and_then(|worker| worker.join().map_err(|_| ()))
            .and_then(|result| result);
        if result.is_err() {
            if std::thread::panicking() {
                eprintln!("Disposable database cleanup failed during test unwinding");
            } else {
                panic!("Disposable database cleanup failed");
            }
        }
    }
}

fn valid_name(name: &str) -> bool {
    name.strip_prefix("test_")
        .is_some_and(|suffix| suffix.len() == 32 && suffix.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn remove_database(admin_url: &str, name: &str) -> Result<(), ()> {
    if !valid_name(name) {
        return Err(());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| ())?;
    let result = runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut connection = PgConnection::connect(admin_url).await.map_err(|_| ())?;
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "DROP DATABASE IF EXISTS {name} WITH (FORCE)"
            )))
            .execute(&mut connection)
            .await
            .map_err(|_| ())?;
            connection.close().await.map_err(|_| ())
        })
        .await
        .map_err(|_| ())?
    });
    runtime.shutdown_timeout(Duration::from_millis(100));
    result
}

#[tokio::test]
async fn fixture_cleanup_removes_only_owned_database_even_with_an_open_connection() {
    let db = super::Database::new().await;
    let survivor = super::Database::new().await;
    let name = db.cleanup.name.clone();
    let mut held = db.pool.acquire().await.unwrap();
    drop(db);
    assert!(sqlx::query("SELECT 1").execute(&mut *held).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pg_database WHERE datname=$1")
            .bind(name)
            .fetch_one(&survivor.pool)
            .await
            .unwrap(),
        0
    );
    survivor.store.close().await;
}

#[tokio::test]
async fn fixture_cleanup_runs_when_a_test_panics() {
    let db = super::Database::new().await;
    let survivor = super::Database::new().await;
    let name = db.cleanup.name.clone();
    let failed = tokio::spawn(async move {
        let _owned = db;
        panic!("synthetic fixture failure");
    });
    assert!(failed.await.unwrap_err().is_panic());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pg_database WHERE datname=$1")
            .bind(name)
            .fetch_one(&survivor.pool)
            .await
            .unwrap(),
        0
    );
    survivor.store.close().await;
}
