use super::Database;
use darkhorse_adapters::postgres::migrations::Error;
use darkhorse_domain::identity::OperationId;
fn id(n: u128) -> OperationId {
    OperationId::from_u128(n).unwrap()
}
async fn history(db: &Database) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM public._sqlx_migrations")
        .fetch_one(&db.pool)
        .await
        .unwrap()
}
async fn initialize_journal(db: &Database) {
    sqlx::raw_sql(include_str!("../../src/postgres/migrations/journal-v1.sql"))
        .execute(&db.pool)
        .await
        .unwrap();
}
#[tokio::test]
async fn fresh_upgrade_and_noop_preserve_baseline_and_step_receipts() {
    for version in [0, 22, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34] {
        let db = Database::at_version(version).await;
        assert!(db.store.inspect_migration(id(1)).await.unwrap().is_none());
        let absent: bool =
            sqlx::query_scalar("SELECT to_regnamespace('darkhorse_migration_v1') IS NULL")
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert!(absent);
        db.store.migrate_operation(id(1)).await.unwrap();
        let record = db.store.inspect_migration(id(1)).await.unwrap().unwrap();
        assert!(record.completed_ms.is_some());
        assert_eq!(record.steps.len(), 34);
        assert_eq!(
            record.steps.iter().filter(|s| s.already_applied).count(),
            version as usize
        );
        assert_eq!(
            record
                .steps
                .iter()
                .filter(|s| s.completed_ms.is_some())
                .count(),
            34 - version as usize
        );
        assert!(record.steps.iter().all(|s| s.current_matches));
        let before = history(&db).await;
        assert_eq!(
            db.store.migrate_operation(id(1)).await,
            Err(Error::Unavailable)
        );
        db.store.migrate_operation(id(2)).await.unwrap();
        let second = db.store.inspect_migration(id(2)).await.unwrap().unwrap();
        assert!(
            second
                .steps
                .iter()
                .all(|s| s.already_applied && s.completed_ms.is_none())
        );
        assert_eq!(history(&db).await, before);
        assert!(db.store.inspect_migration(id(3)).await.unwrap().is_none());
        db.pool.close().await;
    }
}
#[tokio::test]
async fn incompatible_history_is_rejected_before_intent_or_application_changes() {
    for corruption in [
        "UPDATE _sqlx_migrations SET checksum='\\x00' WHERE version=1",
        "UPDATE _sqlx_migrations SET success=false WHERE version=1",
        "UPDATE _sqlx_migrations SET version=99 WHERE version=1",
        "DELETE FROM _sqlx_migrations WHERE version=1",
    ] {
        let db = Database::at_version(22).await;
        sqlx::raw_sql(sqlx::AssertSqlSafe(corruption))
            .execute(&db.pool)
            .await
            .unwrap();
        assert_eq!(
            db.store.migrate_operation(id(1)).await,
            Err(Error::Incompatible)
        );
        assert!(db.store.inspect_migration(id(1)).await.unwrap().is_none());
        let pending: bool =
            sqlx::query_scalar("SELECT to_regclass('limiter_activation_intents') IS NULL")
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert!(pending);
        db.pool.close().await;
    }
}
#[tokio::test]
async fn step_receipt_failure_rolls_back_only_that_migration_and_history() {
    let db = Database::at_version(22).await;
    initialize_journal(&db).await;
    sqlx::raw_sql("CREATE FUNCTION reject_migration_step() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.version=24 THEN RAISE EXCEPTION 'injected'; END IF; RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER reject_migration_step AFTER INSERT ON darkhorse_migration_v1.steps DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_migration_step();").execute(&db.pool).await.unwrap();
    assert_eq!(
        db.store.migrate_operation(id(1)).await,
        Err(Error::Uncertain)
    );
    let record = db.store.inspect_migration(id(1)).await.unwrap().unwrap();
    assert!(record.completed_ms.is_none());
    assert!(record.steps[22].completed_ms.is_some());
    assert!(record.steps[22].current_matches);
    assert!(record.steps[23].completed_ms.is_none());
    assert!(!record.steps[23].current_matches);
    assert_eq!(history(&db).await, 23);
    let rolled_back: bool =
        sqlx::query_scalar("SELECT to_regclass('signing_operation_intents') IS NULL")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert!(rolled_back);
    sqlx::query("DROP TRIGGER reject_migration_step ON darkhorse_migration_v1.steps")
        .execute(&db.pool)
        .await
        .unwrap();
    db.store.migrate_operation(id(2)).await.unwrap();
    let next = db.store.inspect_migration(id(2)).await.unwrap().unwrap();
    assert!(next.completed_ms.is_some());
    assert!(next.steps[22].already_applied);
    assert!(next.steps[22].completed_ms.is_none());
    assert!(next.steps[23].completed_ms.is_some());
    assert!(
        db.store
            .inspect_migration(id(1))
            .await
            .unwrap()
            .unwrap()
            .completed_ms
            .is_none()
    );
    db.pool.close().await;
}
#[tokio::test]
async fn journal_failure_prevents_changes_and_completion_failure_keeps_steps() {
    for target in ["intents", "completions"] {
        let db = Database::at_version(23).await;
        initialize_journal(&db).await;
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION reject_migration_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected'; END $$; CREATE CONSTRAINT TRIGGER reject_migration_commit AFTER INSERT ON darkhorse_migration_v1.{target} DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_migration_commit();"))).execute(&db.pool).await.unwrap();
        assert_eq!(
            db.store.migrate_operation(id(1)).await,
            Err(Error::Uncertain)
        );
        let record = db.store.inspect_migration(id(1)).await.unwrap();
        if target == "intents" {
            assert!(record.is_none());
            assert_eq!(history(&db).await, 23);
        } else {
            let record = record.unwrap();
            assert!(record.completed_ms.is_none());
            assert!(record.steps[23].completed_ms.is_some());
            assert!(record.steps.iter().all(|s| s.current_matches));
            assert_eq!(history(&db).await, 34);
        }
        db.pool.close().await;
    }
}
#[tokio::test]
async fn concurrent_migrators_serialize_and_history_drift_does_not_rewrite_receipts() {
    let db = Database::at_version(22).await;
    let (first, second) = tokio::join!(
        db.store.migrate_operation(id(1)),
        db.store.migrate_operation(id(2))
    );
    first.unwrap();
    second.unwrap();
    let mut applied = 0;
    for n in [1, 2] {
        let record = db.store.inspect_migration(id(n)).await.unwrap().unwrap();
        assert!(record.completed_ms.is_some());
        applied += record
            .steps
            .iter()
            .filter(|s| s.completed_ms.is_some())
            .count();
    }
    assert_eq!(applied, 11);
    sqlx::query("UPDATE _sqlx_migrations SET checksum='\\x00' WHERE version=24")
        .execute(&db.pool)
        .await
        .unwrap();
    let record = db.store.inspect_migration(id(1)).await.unwrap().unwrap();
    assert!(record.completed_ms.is_some());
    assert!(!record.steps[23].current_matches);
    for table in ["intents", "targets", "steps", "completions"] {
        for action in ["DELETE FROM", "TRUNCATE"] {
            assert!(
                sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                    "{action} darkhorse_migration_v1.{table}"
                )))
                .execute(&db.pool)
                .await
                .is_err()
            );
        }
    }
    db.pool.close().await;
}
#[tokio::test]
async fn lost_intent_step_and_final_commit_replies_are_inspectable() {
    for commit in 1..=12 {
        let db = Database::at_version(23).await;
        let (store, proxy) = super::limiter_activation::lost_nth_commit(&db.pool, commit).await;
        assert_eq!(store.migrate_operation(id(1)).await, Err(Error::Uncertain));
        proxy.await.unwrap();
        let record = db.store.inspect_migration(id(1)).await.unwrap().unwrap();
        assert_eq!(record.steps[23].completed_ms.is_some(), commit >= 2);
        assert_eq!(record.steps[23].current_matches, commit >= 2);
        assert_eq!(record.completed_ms.is_some(), commit == 12);
        assert_eq!(history(&db).await, 23 + (commit - 1).min(10) as i64);
        store.close().await;
        db.pool.close().await;
    }
}
#[tokio::test]
async fn inspection_does_not_wait_for_migration_lock_and_cancelled_runner_releases_it() {
    use sqlx::migrate::Migrate;
    let db = Database::at_version(23).await;
    let mut held = db.pool.acquire().await.unwrap();
    held.lock().await.unwrap();
    let store = db.store.clone();
    let worker = tokio::spawn(async move { store.migrate_operation(id(1)).await });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            db.store.inspect_migration(id(1))
        )
        .await
        .unwrap()
        .unwrap()
        .is_none()
    );
    worker.abort();
    assert!(worker.await.unwrap_err().is_cancelled());
    held.unlock().await.unwrap();
    drop(held);
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        db.store.migrate_operation(id(2)),
    )
    .await
    .unwrap()
    .unwrap();
    db.pool.close().await;
}

#[tokio::test]
async fn cancellation_after_intent_rolls_back_open_step_and_releases_session_lock() {
    let db = Database::at_version(23).await;
    initialize_journal(&db).await;
    let mut blocker = db.pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE provider_audit IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let store = db.store.clone();
    let worker = tokio::spawn(async move { store.migrate_operation(id(1)).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND pid<>pg_backend_pid())").fetch_one(&db.pool).await.unwrap();
            let intent: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM darkhorse_migration_v1.intents)").fetch_one(&db.pool).await.unwrap();
            if waiting && intent { break; }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    worker.abort();
    assert!(worker.await.unwrap_err().is_cancelled());
    blocker.rollback().await.unwrap();
    let record = db.store.inspect_migration(id(1)).await.unwrap().unwrap();
    assert!(record.completed_ms.is_none());
    assert!(record.steps[23].completed_ms.is_none());
    assert!(!record.steps[23].current_matches);
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        db.store.migrate_operation(id(2)),
    )
    .await
    .unwrap()
    .unwrap();
    db.pool.close().await;
}

#[tokio::test]
async fn incomplete_existing_journal_is_not_repaired_and_no_migrations_run() {
    let db = Database::at_version(23).await;
    sqlx::query("CREATE SCHEMA darkhorse_migration_v1")
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        db.store.migrate_operation(id(1)).await,
        Err(Error::Unavailable)
    );
    assert_eq!(history(&db).await, 23);
    assert!(matches!(
        db.store.inspect_migration(id(1)).await,
        Err(Error::Unavailable)
    ));
    let untouched: bool =
        sqlx::query_scalar("SELECT to_regclass('darkhorse_migration_v1.intents') IS NULL")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert!(untouched);
    db.pool.close().await;
}

#[tokio::test]
async fn migration_sql_failure_preserves_intent_and_existing_objects() {
    let db = Database::at_version(23).await;
    sqlx::query("CREATE TABLE signing_operation_intents(fixture boolean)")
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        db.store.migrate_operation(id(1)).await,
        Err(Error::Uncertain)
    );
    let record = db.store.inspect_migration(id(1)).await.unwrap().unwrap();
    assert!(record.completed_ms.is_none());
    assert!(record.steps[23].completed_ms.is_none());
    assert!(!record.steps[23].current_matches);
    assert_eq!(history(&db).await, 23);
    sqlx::query("SELECT fixture FROM signing_operation_intents")
        .fetch_all(&db.pool)
        .await
        .unwrap();
    db.pool.close().await;
}
#[tokio::test]
async fn migration_lock_timeout_creates_no_intent_and_does_not_retain_a_session() {
    use sqlx::migrate::Migrate;
    let db = Database::at_version(23).await;
    let mut blocker = db.pool.acquire().await.unwrap();
    blocker.lock().await.unwrap();
    let options = (*db.pool.connect_options())
        .clone()
        .options([("lock_timeout", "50")]);
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    let store = darkhorse_adapters::postgres::PostgresStore::from_pool(pool);
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        store.migrate_operation(id(1)),
    )
    .await
    .unwrap();
    assert_eq!(result, Err(Error::Unavailable));
    assert!(db.store.inspect_migration(id(1)).await.unwrap().is_none());
    blocker.unlock().await.unwrap();
    drop(blocker);
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        db.store.migrate_operation(id(2)),
    )
    .await
    .unwrap()
    .unwrap();
    store.close().await;
    db.pool.close().await;
}

#[tokio::test]
async fn a_suppressed_final_receipt_cannot_report_batch_success() {
    let db = Database::at_version(23).await;
    initialize_journal(&db).await;
    sqlx::raw_sql("CREATE FUNCTION suppress_completion() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NULL; END $$; CREATE TRIGGER suppress_completion BEFORE INSERT ON darkhorse_migration_v1.completions FOR EACH ROW EXECUTE FUNCTION suppress_completion();").execute(&db.pool).await.unwrap();
    assert_eq!(
        db.store.migrate_operation(id(1)).await,
        Err(Error::Uncertain)
    );
    let record = db.store.inspect_migration(id(1)).await.unwrap().unwrap();
    assert!(record.completed_ms.is_none());
    assert!(record.steps[23].completed_ms.is_some());
    assert!(record.steps[23].current_matches);
    db.pool.close().await;
}
