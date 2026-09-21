use super::Database;
use darkhorse_adapters::readiness::Readiness;
#[tokio::test]
async fn primary_probe_uses_the_existing_pool_without_security_writes() {
    let db = Database::new().await;
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM security_audit")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert!(db.store.ready().await);
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM security_audit")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    db.pool.close().await;
    assert!(!db.store.ready().await);
}
