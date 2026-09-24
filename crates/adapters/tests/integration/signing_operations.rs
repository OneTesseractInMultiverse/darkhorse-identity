use super::{Database, limiter_activation::lost_commit, signing::key};
use darkhorse_application::{signing::SigningStore, signing_operations::*};
use darkhorse_domain::{
    identity::OperationId,
    signing::{KeyError, Phase},
};
fn intent(n: u128, kind: Kind, revision: u64, k: u8) -> Intent {
    Intent {
        id: OperationId::from_u128(n).unwrap(),
        issuer: "https://issuer.example".into(),
        kid: key(k).public.kid,
        kind,
        expected_revision: revision,
    }
}
#[tokio::test]
async fn binding_mutation_audit_and_receipt_commit_together() {
    let db = Database::new().await;
    let request = intent(1, Kind::Generate, 0, 1);
    db.store.prepare_signing(&request).await.unwrap();
    let pending = db.store.inspect_signing(request.id).await.unwrap();
    assert!(pending.completion.is_none());
    assert_eq!(pending.current_revision, None);
    assert_eq!(
        db.store
            .complete_signing(&request, [1; 32], Some(key(1)))
            .await,
        Ok(1)
    );
    let completed = db.store.inspect_signing(request.id).await.unwrap();
    assert_eq!(completed.current_phase, Some(Phase::Staged));
    assert_eq!(completed.completion.as_ref().unwrap().revision, 1);
    assert!(completed.completion.unwrap().completed_ms >= pending.prepared_ms);
    let retire = intent(2, Kind::Retire, 1, 1);
    assert_eq!(execute(&db.store, &retire, [1; 32], None).await, Ok(2));
    let historical = db.store.inspect_signing(request.id).await.unwrap();
    assert_eq!(historical.current_phase, Some(Phase::Retired));
    assert_eq!(historical.current_revision, Some(2));
    assert_eq!(historical.completion.unwrap().revision, 1);
    assert!(db.store.prepare_signing(&request).await.is_err());
    assert!(
        db.store
            .complete_signing(&request, [1; 32], Some(key(1)))
            .await
            .is_err()
    );
    for sql in [
        "UPDATE signing_operation_intents SET kid=kid",
        "DELETE FROM signing_operation_intents",
        "UPDATE signing_operation_receipts SET audit_id=audit_id",
        "DELETE FROM signing_operation_receipts",
    ] {
        assert!(sqlx::query(sql).execute(&db.pool).await.is_err());
    }
    db.store.close().await;
}
#[tokio::test]
async fn missing_intent_mismatched_target_material_revision_or_binding_never_mutates() {
    let db = Database::new().await;
    let request = intent(1, Kind::Import, 0, 1);
    assert!(
        db.store
            .complete_signing(&request, [1; 32], Some(key(1)))
            .await
            .is_err()
    );
    assert_eq!(
        db.store.inspect_signing(request.id).await,
        Err(Error::Rejected(KeyError::NotFound))
    );
    db.store.prepare_signing(&request).await.unwrap();
    for changed in [
        Intent {
            kid: key(2).public.kid,
            ..request.clone()
        },
        Intent {
            kind: Kind::Generate,
            ..request.clone()
        },
        Intent {
            expected_revision: 1,
            ..request.clone()
        },
        Intent {
            issuer: "https://other.example".into(),
            ..request.clone()
        },
    ] {
        assert!(
            db.store
                .complete_signing(&changed, [1; 32], Some(key(1)))
                .await
                .is_err()
        );
    }
    assert!(
        db.store
            .complete_signing(&request, [1; 32], Some(key(2)))
            .await
            .is_err()
    );
    assert!(
        db.store
            .complete_signing(&request, [1; 32], None)
            .await
            .is_err()
    );
    assert_eq!(
        db.store
            .inspect_signing(request.id)
            .await
            .unwrap()
            .current_revision,
        None
    );
    db.store
        .complete_signing(&request, [1; 32], Some(key(1)))
        .await
        .unwrap();
    let retire = intent(2, Kind::Retire, 1, 1);
    db.store.prepare_signing(&retire).await.unwrap();
    assert!(
        db.store
            .complete_signing(&retire, [1; 32], Some(key(1)))
            .await
            .is_err()
    );
    assert_eq!(
        db.store.complete_signing(&retire, [2; 32], None).await,
        Err(Error::Rejected(KeyError::Conflict))
    );
    let activate = intent(3, Kind::Activate, 1, 1);
    assert_eq!(
        execute(&db.store, &activate, [1; 32], None).await,
        Err(Error::Rejected(KeyError::NotReady))
    );
    assert_eq!(
        db.store.inventory(&request.issuer).await.unwrap().revision,
        1
    );
    db.store.close().await;
}
#[tokio::test]
async fn audit_or_receipt_failure_rolls_back_initial_binding_and_key() {
    for table in ["provider_audit", "signing_operation_receipts"] {
        let db = Database::new().await;
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION reject_signing() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected'; END $$; CREATE TRIGGER reject_signing BEFORE INSERT ON {table} FOR EACH ROW EXECUTE FUNCTION reject_signing();"))).execute(&db.pool).await.unwrap();
        let request = intent(1, Kind::Generate, 0, 1);
        assert_eq!(
            execute(&db.store, &request, [1; 32], Some(key(1))).await,
            Err(Error::Rejected(KeyError::Unavailable))
        );
        let pending = db.store.inspect_signing(request.id).await.unwrap();
        assert!(pending.completion.is_none());
        assert_eq!(pending.current_revision, None);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT (SELECT count(*) FROM signing_keys)+(SELECT count(*) FROM provider_audit)"
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
async fn competing_operations_cannot_share_a_revision_or_completion() {
    let db = Database::new().await;
    let a = intent(1, Kind::Generate, 0, 1);
    let b = intent(2, Kind::Import, 0, 2);
    db.store.prepare_signing(&a).await.unwrap();
    db.store.prepare_signing(&b).await.unwrap();
    let (x, y) = tokio::join!(
        db.store.complete_signing(&a, [1; 32], Some(key(1))),
        db.store.complete_signing(&b, [1; 32], Some(key(2)))
    );
    assert_eq!(usize::from(x.is_ok()) + usize::from(y.is_ok()), 1);
    assert!(
        x == Err(Error::Rejected(KeyError::Conflict))
            || y == Err(Error::Rejected(KeyError::Conflict))
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM signing_operation_receipts")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    db.store.close().await;
}
#[tokio::test]
async fn lost_intent_and_completion_replies_remain_inspectable_without_retry() {
    for completion in [false, true] {
        let db = Database::new().await;
        let request = intent(1, Kind::Generate, 0, 1);
        if completion {
            db.store.prepare_signing(&request).await.unwrap();
        }
        let (store, proxy) = lost_commit(&db.pool).await;
        let result = if completion {
            store
                .complete_signing(&request, [1; 32], Some(key(1)))
                .await
                .map(|_| ())
        } else {
            store.prepare_signing(&request).await
        };
        assert_eq!(result, Err(Error::Uncertain));
        proxy.await.unwrap();
        let record = db.store.inspect_signing(request.id).await.unwrap();
        assert_eq!(record.completion.is_some(), completion);
        assert_eq!(record.current_revision, completion.then_some(1));
        store.close().await;
        db.store.close().await;
    }
}
#[tokio::test]
async fn failed_commit_is_uncertain_and_upgrade_invents_no_history() {
    let db = Database::at_version(23).await;
    let request = intent(1, Kind::Generate, 0, 1);
    assert!(db.store.prepare_signing(&request).await.is_err());
    db.store.migrate().await.unwrap();
    assert_eq!(
        db.store.inspect_signing(request.id).await,
        Err(Error::Rejected(KeyError::NotFound))
    );
    sqlx::raw_sql("CREATE FUNCTION reject_signing_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected'; END $$; CREATE CONSTRAINT TRIGGER reject_signing_commit AFTER INSERT ON signing_operation_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_signing_commit();").execute(&db.pool).await.unwrap();
    assert_eq!(
        execute(&db.store, &request, [1; 32], Some(key(1))).await,
        Err(Error::Uncertain)
    );
    let pending = db.store.inspect_signing(request.id).await.unwrap();
    assert!(pending.completion.is_none());
    assert_eq!(pending.current_revision, None);
    db.store.close().await;
}
#[tokio::test]
async fn journaled_rotation_preserves_publication_and_verification_waits() {
    let db = Database::new().await;
    let first = intent(1, Kind::Generate, 0, 1);
    execute(&db.store, &first, [1; 32], Some(key(1)))
        .await
        .unwrap();
    super::signing::elapsed(&db, &first.kid).await;
    let activate = intent(2, Kind::Activate, 1, 1);
    assert_eq!(execute(&db.store, &activate, [1; 32], None).await, Ok(2));
    let second = intent(3, Kind::Import, 2, 2);
    execute(&db.store, &second, [1; 32], Some(key(2)))
        .await
        .unwrap();
    super::signing::elapsed(&db, &second.kid).await;
    execute(&db.store, &intent(4, Kind::Activate, 3, 2), [1; 32], None)
        .await
        .unwrap();
    assert_eq!(
        execute(&db.store, &intent(5, Kind::Retire, 4, 1), [1; 32], None).await,
        Err(Error::Rejected(KeyError::NotReady))
    );
    sqlx::raw_sql("ALTER TABLE signing_keys DISABLE TRIGGER signing_transition; UPDATE signing_keys SET created_ms=created_ms-600001,activated_ms=activated_ms-600001,verify_until_ms=verify_until_ms-600001 WHERE phase='retiring'; ALTER TABLE signing_keys ENABLE TRIGGER signing_transition;").execute(&db.pool).await.unwrap();
    assert_eq!(
        execute(&db.store, &intent(6, Kind::Retire, 4, 1), [1; 32], None).await,
        Ok(5)
    );
    let history = db.store.inspect_signing(activate.id).await.unwrap();
    assert_eq!(history.completion.unwrap().revision, 2);
    assert_eq!(history.current_phase, Some(Phase::Retired));
    assert_eq!(history.current_revision, Some(5));
    assert_eq!(db.store.published(&first.issuer).await.unwrap().len(), 1);
    db.store.close().await;
}
