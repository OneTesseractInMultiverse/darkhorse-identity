use darkhorse_application::directory::DirectoryStore;
use darkhorse_application::{authentication::AuthenticationStore, email_verification::*};
use darkhorse_domain::{email_verification::Error, identity::EmailVerificationId};
fn material(n: u8) -> Material {
    Material {
        id: EmailVerificationId::from_u128(n.into()).unwrap(),
        seed: [n; 32],
        digest: [n; 32],
    }
}
#[tokio::test]
async fn email_proof_is_live_owner_bound_single_use_and_audited() {
    let db = super::oidc::fixture().await;
    assert!(!db.store.email_status([1; 32]).await.unwrap().verified);
    assert_eq!(
        db.store.request_verification([0; 32], material(3)).await,
        Err(Error::Unauthorized)
    );
    db.store
        .request_verification([1; 32], material(3))
        .await
        .unwrap();
    assert_eq!(
        db.store.request_verification([1; 32], material(4)).await,
        Err(Error::Throttled)
    );
    let job = db.store.claim_email().await.unwrap().unwrap();
    assert_eq!(job.email, "one@example.com");
    assert_eq!(job.attempt, 1);
    assert!(db.store.claim_email().await.unwrap().is_none());
    db.store
        .finish_email(job.id, job.attempt, DeliveryResult::Accepted)
        .await
        .unwrap();
    assert_eq!(
        db.store.verify_email([0; 32], [3; 32]).await,
        Err(Error::Unauthorized)
    );
    assert_eq!(
        db.store.verify_email([1; 32], [99; 32]).await,
        Err(Error::Invalid)
    );
    let (a, b) = tokio::join!(
        db.store.verify_email([1; 32], [3; 32]),
        db.store.verify_email([1; 32], [3; 32])
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(db.store.email_status([1; 32]).await.unwrap().verified);
    assert_eq!(
        db.store.verify_email([1; 32], [3; 32]).await,
        Err(Error::Invalid)
    );
    let events: Vec<String> =
        sqlx::query_scalar("SELECT event FROM email_verification_audit ORDER BY id")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(events, ["requested", "accepted", "verified"]);
    let seed: Option<Vec<u8>> = sqlx::query_scalar("SELECT seed FROM email_verifications")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert!(seed.is_none());
    sqlx::query("UPDATE principals SET email='new@example.com',revision=revision+1")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(!db.store.email_status([1; 32]).await.unwrap().verified);
    db.store.close().await;
}
#[tokio::test]
async fn changed_epoch_wrong_owner_or_changed_email_cannot_redeem_and_delivery_is_cancelled() {
    for change in ["epoch", "email", "inactive", "owner"] {
        let db = super::oidc::fixture().await;
        db.store
            .request_verification([1; 32], material(3))
            .await
            .unwrap();
        super::insert_principal(&db, 2, true).await;
        if change == "epoch" {
            sqlx::query("UPDATE principals SET credential_epoch=credential_epoch+1,revision=revision+1 WHERE id=$1").bind(uuid::Uuid::from_u128(1)).execute(&db.pool).await.unwrap();
        }
        if change == "email" {
            sqlx::query(
                "UPDATE principals SET email='changed@example.com',revision=revision+1 WHERE id=$1",
            )
            .bind(uuid::Uuid::from_u128(1))
            .execute(&db.pool)
            .await
            .unwrap();
            sqlx::query(
                "UPDATE principals SET email='one@example.com',revision=revision+1 WHERE id=$1",
            )
            .bind(uuid::Uuid::from_u128(1))
            .execute(&db.pool)
            .await
            .unwrap();
        }
        if change == "inactive" {
            db.store
                .change(
                    super::id(1),
                    0,
                    darkhorse_domain::directory::AccountAction::SetStatus(
                        darkhorse_domain::AccountStatus::Inactive,
                    ),
                )
                .await
                .unwrap();
        }
        let actor = if change == "owner" {
            let c = db
                .store
                .candidate("person2@example.com")
                .await
                .unwrap()
                .unwrap();
            db.store.establish(&c, [2; 32], None).await.unwrap();
            [2; 32]
        } else {
            [1; 32]
        };
        assert!(db.store.verify_email(actor, [3; 32]).await.is_err());
        if change != "owner" {
            assert!(db.store.claim_email().await.unwrap().is_none());
        }
        db.store.close().await;
    }
}
#[tokio::test]
async fn audit_failure_rolls_back_proof_confirmation_and_delivery_acknowledgement() {
    let db = super::oidc::fixture().await;
    sqlx::raw_sql("CREATE FUNCTION reject_email_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture failure'; END; $$; CREATE TRIGGER reject_email_audit BEFORE INSERT ON email_verification_audit FOR EACH ROW EXECUTE FUNCTION reject_email_audit();").execute(&db.pool).await.unwrap();
    assert_eq!(
        db.store.request_verification([1; 32], material(3)).await,
        Err(Error::Unavailable)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM email_verifications")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query("DROP TRIGGER reject_email_audit ON email_verification_audit")
        .execute(&db.pool)
        .await
        .unwrap();
    db.store
        .request_verification([1; 32], material(3))
        .await
        .unwrap();
    let job = db.store.claim_email().await.unwrap().unwrap();
    sqlx::query("CREATE TRIGGER reject_email_audit BEFORE INSERT ON email_verification_audit FOR EACH ROW EXECUTE FUNCTION reject_email_audit()").execute(&db.pool).await.unwrap();
    assert_eq!(
        db.store.verify_email([1; 32], [3; 32]).await,
        Err(Error::Unavailable)
    );
    assert!(!db.store.email_status([1; 32]).await.unwrap().verified);
    assert_eq!(
        db.store
            .finish_email(job.id, job.attempt, DeliveryResult::Accepted)
            .await,
        Err(Error::Unavailable)
    );
    let current: (String, bool, bool) =
        sqlx::query_as("SELECT delivery_state,consumed,seed IS NOT NULL FROM email_verifications")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(current, ("queued".into(), false, true));
    db.store.close().await;
}
#[tokio::test]
async fn queued_jobs_use_exclusive_leases_stale_ack_fencing_and_bounded_retry() {
    let db = super::oidc::fixture().await;
    db.store
        .request_verification([1; 32], material(3))
        .await
        .unwrap();
    let (left, right) = tokio::join!(db.store.claim_email(), db.store.claim_email());
    assert_eq!(
        usize::from(left.unwrap().is_some()) + usize::from(right.unwrap().is_some()),
        1
    );
    for attempt in 1..=5 {
        db.store
            .finish_email(material(3).id, attempt, DeliveryResult::Retry)
            .await
            .unwrap();
        assert!(db.store.claim_email().await.unwrap().is_none());
        if attempt < 5 {
            sqlx::query("UPDATE email_verifications SET next_ms=created_ms")
                .execute(&db.pool)
                .await
                .unwrap();
            let job = db.store.claim_email().await.unwrap().unwrap();
            assert_eq!(job.attempt, attempt + 1);
            db.store
                .finish_email(job.id, attempt, DeliveryResult::Accepted)
                .await
                .unwrap();
            assert_eq!(
                sqlx::query_scalar::<_, String>("SELECT delivery_state FROM email_verifications")
                    .fetch_one(&db.pool)
                    .await
                    .unwrap(),
                "queued"
            );
        }
    }
    let final_state: (String, bool) =
        sqlx::query_as("SELECT delivery_state,seed IS NULL FROM email_verifications")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(final_state, ("failed".into(), true));
    db.store.close().await;
}
#[tokio::test]
async fn deployment_key_and_origin_binding_is_immutable() {
    let db = super::oidc::fixture().await;
    db.store
        .bind_email_delivery("https://identity.example.com", [1; 32])
        .await
        .unwrap();
    db.store
        .bind_email_delivery("https://identity.example.com", [1; 32])
        .await
        .unwrap();
    assert!(
        db.store
            .bind_email_delivery("https://other.example.com", [1; 32])
            .await
            .is_err()
    );
    assert!(
        db.store
            .bind_email_delivery("https://identity.example.com", [2; 32])
            .await
            .is_err()
    );
    for sql in [
        "DELETE FROM email_delivery_state",
        "UPDATE email_delivery_state SET origin='https://other.example.com'",
        "UPDATE email_verification_audit SET event='verified'",
    ] {
        // The audit mutation requires an existing row for its row trigger.
        if sql.contains("audit") {
            db.store
                .request_verification([1; 32], material(3))
                .await
                .unwrap();
        }
        assert!(sqlx::query(sql).execute(&db.pool).await.is_err());
    }
    db.store.close().await;
}
async fn expired_fixture(db: &super::Database) {
    sqlx::query("INSERT INTO email_verifications(principal_id,id,actor_session_id,email,credential_epoch,digest,seed,created_ms,expires_ms,next_ms) SELECT p.id,$1,s.public_id,p.email,p.credential_epoch,$2,$3,n-900001,n-1,n-900001 FROM principals p JOIN browser_sessions s ON s.principal_id=p.id CROSS JOIN (SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint AS n) t WHERE s.digest=$4")
        .bind(uuid::Uuid::from_u128(3)).bind([3u8;32].as_slice()).bind([3u8;32].as_slice()).bind([1u8;32].as_slice()).execute(&db.pool).await.unwrap();
}
#[tokio::test]
async fn expired_replaced_and_terminal_proofs_cannot_be_revived() {
    let db = super::oidc::fixture().await;
    expired_fixture(&db).await;
    assert_eq!(
        db.store.verify_email([1; 32], [3; 32]).await,
        Err(Error::Invalid)
    );
    assert!(db.store.claim_email().await.unwrap().is_none());
    db.store
        .request_verification([1; 32], material(4))
        .await
        .unwrap();
    assert_eq!(
        db.store.verify_email([1; 32], [3; 32]).await,
        Err(Error::Invalid)
    );
    db.store
        .finish_email(material(3).id, 1, DeliveryResult::Accepted)
        .await
        .unwrap();
    let job = db.store.claim_email().await.unwrap().unwrap();
    assert_eq!(job.id, material(4).id);
    for query in [
        "UPDATE email_verifications SET expires_ms=expires_ms+1",
        "UPDATE email_verifications SET digest=repeat('x',32)::bytea",
        "UPDATE email_verifications SET email='other@example.com'",
        "DELETE FROM email_verifications",
    ] {
        assert!(sqlx::query(query).execute(&db.pool).await.is_err());
    }
    db.store
        .finish_email(job.id, job.attempt, DeliveryResult::Rejected)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT delivery_state FROM email_verifications")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        "failed"
    );
    db.store.verify_email([1; 32], [4; 32]).await.unwrap();
    assert!(
        sqlx::query("UPDATE email_verifications SET consumed=false")
            .execute(&db.pool)
            .await
            .is_err()
    );
    db.store
        .request_verification([1; 32], material(5))
        .await
        .unwrap();
    assert!(db.store.claim_email().await.unwrap().is_none());
    db.store.close().await;
    assert_eq!(
        db.store.email_status([1; 32]).await,
        Err(Error::Unavailable)
    );
    assert!(db.store.claim_email().await.is_err());
    assert!(
        db.store
            .finish_email(material(3).id, 1, DeliveryResult::Accepted)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn persistent_daily_budget_survives_process_state_and_blocks_parallel_resends() {
    let db = super::oidc::fixture().await;
    sqlx::query("INSERT INTO email_verification_audit(principal_id,verification_id,event,occurred_ms) SELECT p.id,$1,'requested',floor(extract(epoch FROM clock_timestamp())*1000)::bigint-900001 FROM principals p CROSS JOIN generate_series(1,5)").bind(uuid::Uuid::from_u128(3)).execute(&db.pool).await.unwrap();
    assert_eq!(
        db.store.request_verification([1; 32], material(3)).await,
        Err(Error::Throttled)
    );
    assert!(db.store.claim_email().await.unwrap().is_none());
    db.store.close().await;
    let db = super::oidc::fixture().await;
    let (a, b) = tokio::join!(
        db.store.request_verification([1; 32], material(3)),
        db.store.request_verification([1; 32], material(4))
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    db.store.close().await;
}
