use super::Database;

#[tokio::test]
async fn prepared_plans_refresh_outside_transactions_and_fail_without_transaction_replay() {
    let db = Database::new().await;
    let mut connection = db.pool.acquire().await.unwrap();
    sqlx::raw_sql("CREATE TABLE dependency_plan_probe(value integer NOT NULL); INSERT INTO dependency_plan_probe VALUES (1)")
        .execute(&mut *connection).await.unwrap();
    let sql = "SELECT value FROM dependency_plan_probe";
    assert_eq!(
        sqlx::query_scalar::<_, i32>(sql)
            .fetch_one(&mut *connection)
            .await
            .unwrap(),
        1
    );
    sqlx::query("ALTER TABLE dependency_plan_probe ALTER value TYPE bigint")
        .execute(&mut *connection)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(sql)
            .fetch_one(&mut *connection)
            .await
            .unwrap(),
        1
    );
    use sqlx::Connection;
    let mut transaction = connection.begin().await.unwrap();
    sqlx::query("ALTER TABLE dependency_plan_probe ALTER value TYPE integer")
        .execute(&mut *transaction)
        .await
        .unwrap();
    let error = sqlx::query(sql)
        .fetch_one(&mut *transaction)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("0A000")
    );
    let error = sqlx::query("SELECT 1")
        .fetch_one(&mut *transaction)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("25P02")
    );
    transaction.rollback().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(sql)
            .fetch_one(&mut *connection)
            .await
            .unwrap(),
        1
    );
    drop(connection);
    db.pool.close().await;
}
