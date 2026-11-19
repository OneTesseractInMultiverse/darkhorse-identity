use super::*;
use darkhorse_application::oidc::Preauthorization;

async fn start(db: &Database, request: Request, session: Option<[u8; 32]>) -> Preauthorization {
    match db.store.begin(request, [80; 32], session).await.unwrap() {
        Outcome::Login(pending) => *pending,
        _ => panic!("expected stateless login"),
    }
}
async fn count(db: &Database) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM authorization_requests")
        .fetch_one(&db.pool)
        .await
        .unwrap()
}
#[tokio::test]
async fn missing_forged_and_revoked_sessions_cannot_allocate_or_approve() {
    let db = fixture().await;
    db.store.logout([1; 32]).await.unwrap();
    for session in [None, Some([99; 32]), Some([1; 32])] {
        let prelogin = start(&db, request(), session).await;
        pending(
            db.store
                .continue_login(prelogin.clone(), [80; 32], session, Decision::Inspect)
                .await
                .unwrap(),
            Interaction::Login,
        );
        for decision in [Decision::Approve, Decision::Deny] {
            assert!(matches!(
                db.store
                    .continue_login(prelogin.clone(), [80; 32], session, decision)
                    .await,
                Err(Error::LoginRequired)
            ));
        }
        assert_eq!(count(&db).await, 0);
    }
    db.store.close().await;
}
#[tokio::test]
async fn forced_login_retains_original_time_and_requires_a_fresh_session() {
    let db = fixture().await;
    for (n, prompt, age) in [
        (2u8, Prompt::Login, None),
        (3, Prompt::Default, Some(0)),
        (4, Prompt::LoginConsent, Some(0)),
    ] {
        let mut r = request();
        r.prompt = prompt;
        r.max_age = age;
        let prelogin = start(&db, r, Some([1; 32])).await;
        pending(
            db.store
                .continue_login(prelogin.clone(), [n; 32], Some([1; 32]), Decision::Inspect)
                .await
                .unwrap(),
            Interaction::Login,
        );
        assert_eq!(count(&db).await, i64::from(n - 2));
        let candidate = db
            .store
            .candidate("one@example.com")
            .await
            .unwrap()
            .unwrap();
        db.store.establish(&candidate, [n; 32], None).await.unwrap();
        pending(
            db.store
                .continue_login(prelogin.clone(), [n; 32], Some([n; 32]), Decision::Inspect)
                .await
                .unwrap(),
            Interaction::Consent,
        );
        let (created, expires): (i64, i64) = sqlx::query_as(
            "SELECT created_ms,expires_ms FROM authorization_requests WHERE digest=$1",
        )
        .bind([n; 32].as_slice())
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(created as u64, prelogin.created_ms);
        assert_eq!(expires as u64, prelogin.created_ms + 300_000);
    }
    db.store.close().await;
}
#[tokio::test]
async fn expired_future_and_changed_registration_continuations_never_materialize() {
    let db = fixture().await;
    let original = start(&db, request(), None).await;
    for created_ms in [original.created_ms - 300_001, original.created_ms + 300_000] {
        let mut expired = original.clone();
        expired.created_ms = created_ms;
        assert!(matches!(
            db.store
                .continue_login(expired, [80; 32], Some([1; 32]), Decision::Inspect)
                .await,
            Err(Error::InvalidTransaction)
        ));
    }
    sqlx::query("UPDATE oauth_clients SET revision=revision+1")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(matches!(
        db.store
            .continue_login(original, [80; 32], Some([1; 32]), Decision::Inspect)
            .await,
        Err(Error::InvalidTransaction)
    ));
    assert_eq!(count(&db).await, 0);
    db.store.close().await;
}
#[tokio::test]
async fn repeated_promotion_is_atomic_and_full_capacity_does_not_block_existing_flow() {
    let db = fixture().await;
    let prelogin = start(&db, request(), None).await;
    let (left, right) = tokio::join!(
        db.store
            .continue_login(prelogin.clone(), [80; 32], Some([1; 32]), Decision::Inspect),
        db.store
            .continue_login(prelogin.clone(), [80; 32], Some([1; 32]), Decision::Inspect),
    );
    pending(left.unwrap(), Interaction::Consent);
    pending(right.unwrap(), Interaction::Consent);
    assert_eq!(count(&db).await, 1);
    for n in 100..199u8 {
        db.store
            .begin(request(), [n; 32], Some([1; 32]))
            .await
            .unwrap();
    }
    assert_eq!(count(&db).await, 100);
    let mut held_capacity = db.pool.begin().await.unwrap();
    sqlx::query("SELECT singleton FROM authorization_capacity WHERE singleton FOR UPDATE")
        .fetch_one(&mut *held_capacity)
        .await
        .unwrap();
    let independent = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        db.store
            .continue_login(prelogin.clone(), [80; 32], Some([1; 32]), Decision::Inspect),
    )
    .await
    .expect("an existing continuation must not wait for global admission")
    .unwrap();
    pending(independent, Interaction::Consent);
    held_capacity.rollback().await.unwrap();
    pending(
        db.store
            .continue_login(prelogin.clone(), [80; 32], Some([1; 32]), Decision::Inspect)
            .await
            .unwrap(),
        Interaction::Consent,
    );
    assert!(matches!(
        db.store
            .continue_login(prelogin.clone(), [81; 32], Some([1; 32]), Decision::Inspect)
            .await,
        Err(Error::Unavailable)
    ));
    assert!(matches!(
        db.store
            .continue_login(prelogin.clone(), [80; 32], Some([1; 32]), Decision::Deny)
            .await
            .unwrap(),
        Outcome::Return {
            error: Error::AccessDenied,
            ..
        }
    ));
    assert!(matches!(
        db.store
            .continue_login(prelogin, [80; 32], Some([1; 32]), Decision::Inspect)
            .await,
        Err(Error::InvalidTransaction)
    ));
    assert_eq!(count(&db).await, 100);
    db.store.close().await;
}
#[tokio::test]
async fn racing_sessions_cannot_rebind_a_materialized_continuation() {
    let db = fixture().await;
    let prelogin = start(&db, request(), None).await;
    let candidate = db
        .store
        .candidate("one@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [2; 32], None).await.unwrap();
    let (left, right) = tokio::join!(
        db.store
            .continue_login(prelogin.clone(), [80; 32], Some([1; 32]), Decision::Inspect),
        db.store
            .continue_login(prelogin, [80; 32], Some([2; 32]), Decision::Inspect),
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    assert!(matches!(
        (left, right),
        (Ok(_), Err(Error::InvalidTransaction)) | (Err(Error::InvalidTransaction), Ok(_))
    ));
    assert_eq!(count(&db).await, 1);
    db.store.close().await;
}
#[tokio::test]
async fn resource_policy_is_frozen_but_unrelated_changes_do_not_invalidate_identity_requests() {
    let db = fixture().await;
    sqlx::raw_sql("INSERT INTO protected_resources(id,application_id,name,audience) VALUES('00000000-0000-0000-0000-000000000030','00000000-0000-0000-0000-000000000010','API','urn:darkhorse:resource:00000000-0000-0000-0000-000000000030'); INSERT INTO client_resources VALUES('00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000020','00000000-0000-0000-0000-000000000030');").execute(&db.pool).await.unwrap();
    let identity = start(&db, request(), None).await;
    let mut resource = request();
    resource.resource = Some("urn:darkhorse:resource:00000000-0000-0000-0000-000000000030".into());
    let resource = start(&db, resource, None).await;
    sqlx::query("UPDATE security_state SET policy_revision=policy_revision+1 WHERE singleton")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(matches!(
        db.store
            .continue_login(resource, [80; 32], Some([1; 32]), Decision::Inspect)
            .await,
        Err(Error::InvalidTransaction)
    ));
    pending(
        db.store
            .continue_login(identity, [81; 32], Some([1; 32]), Decision::Inspect)
            .await
            .unwrap(),
        Interaction::Consent,
    );
    db.store.close().await;
}
