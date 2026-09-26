use super::*;
use std::time::Duration;

#[tokio::test]
async fn account_read_shares_security_fence_and_does_not_take_a_target_update_lock() {
    let db = fixture().await;
    let mut blocker = db.pool.begin().await.unwrap();
    sqlx::query("SELECT singleton FROM security_state FOR SHARE")
        .execute(&mut *blocker)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM principals WHERE id=$1 FOR NO KEY UPDATE")
        .bind(Uuid::from_u128(2))
        .execute(&mut *blocker)
        .await
        .unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        execute(&db.store, "admin@example.com", Operation::Show(id(2))),
    )
    .await;
    blocker.rollback().await.unwrap();
    assert_eq!(result.expect("read must complete while another reader holds the fence and target has only a non-key update lock").unwrap().account.id,id(2));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM operator_account_audit WHERE result='read'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        1
    );
}

async fn wait_for_lock(db: &Database, statement: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE $1)")
                .bind(statement).fetch_one(&db.pool).await.unwrap();
            if waiting { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("operation must reach the deliberately held lock");
}

#[tokio::test]
async fn account_read_keeps_its_shared_fence_through_audit_and_excludes_security_writers() {
    let db = fixture().await;
    sqlx::raw_sql("CREATE FUNCTION hold_read_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(32,(SELECT oid::int FROM pg_database WHERE datname=current_database())); RETURN NEW; END $$; CREATE TRIGGER hold_read_audit BEFORE INSERT ON operator_account_audit FOR EACH ROW EXECUTE FUNCTION hold_read_audit();").execute(&db.pool).await.unwrap();
    let mut gate = db.pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(32,(SELECT oid::int FROM pg_database WHERE datname=current_database()))").execute(&mut *gate).await.unwrap();
    let store = db.store.clone();
    let reading =
        tokio::spawn(
            async move { execute(&store, "admin@example.com", Operation::Show(id(2))).await },
        );
    wait_for_lock(&db, "%INSERT INTO operator_account_audit%").await;

    let mut other_reader = db.pool.begin().await.unwrap();
    let shared = sqlx::query("SELECT singleton FROM security_state FOR SHARE NOWAIT")
        .execute(&mut *other_reader)
        .await;
    other_reader.rollback().await.unwrap();
    let mut writer = db.pool.begin().await.unwrap();
    let exclusive = sqlx::query("SELECT singleton FROM security_state FOR UPDATE NOWAIT")
        .execute(&mut *writer)
        .await;
    writer.rollback().await.unwrap();
    gate.commit().await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), reading)
        .await
        .unwrap()
        .unwrap();
    assert!(
        shared.is_ok(),
        "another reader must share the fence during audit"
    );
    assert_eq!(
        exclusive
            .unwrap_err()
            .as_database_error()
            .unwrap()
            .code()
            .as_deref(),
        Some("55P03")
    );
    assert!(result.is_ok());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM operator_account_audit WHERE result='read'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        1
    );
    // The write fence becomes available only after the audited read completes.
    sqlx::query("SELECT singleton FROM security_state FOR UPDATE NOWAIT")
        .execute(&db.pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn account_detail_reads_the_post_writer_snapshot_after_waiting_for_the_fence() {
    let db = fixture().await;
    let mut writer = db.pool.begin().await.unwrap();
    sqlx::query("SELECT singleton FROM security_state FOR UPDATE")
        .execute(&mut *writer)
        .await
        .unwrap();
    let store = db.store.clone();
    let reading =
        tokio::spawn(
            async move { execute(&store, "admin@example.com", Operation::Show(id(2))).await },
        );
    wait_for_lock(&db, "%security_state%").await;
    sqlx::query("UPDATE principals SET first_name='After',revision=revision+1 WHERE id=$1")
        .bind(Uuid::from_u128(2))
        .execute(&mut *writer)
        .await
        .unwrap();
    writer.commit().await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), reading)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result.account.profile.first_name(), "After");
    assert_eq!(result.account.revision, 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT target_revision FROM operator_account_audit WHERE result='read'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        1
    );
}
