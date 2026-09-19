use super::{Database, administrator};
use darkhorse_application::{
    authentication::{AuthError, AuthenticationStore},
    bootstrap::BootstrapStore,
    directory::DirectoryStore,
};
use darkhorse_domain::directory::AccountAction;

#[tokio::test]
async fn credential_changes_during_issuance_and_inactive_accounts_never_create_sessions() {
    let db = Database::new().await;
    let principal = db
        .store
        .bootstrap(administrator(92, "race@example.com"))
        .await
        .unwrap();
    let candidate = db
        .store
        .candidate("race@example.com")
        .await
        .unwrap()
        .unwrap();
    let mut tx = db.pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE principals SET credential_epoch=credential_epoch+1,revision=revision+1 WHERE id=$1",
    )
    .bind(uuid::Uuid::from_u128(principal.as_u128()))
    .execute(&mut *tx)
    .await
    .unwrap();
    let store = db.store.clone();
    let mut blocked = tokio::spawn(async move { store.establish(&candidate, [8; 32], None).await });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut blocked)
            .await
            .is_err()
    );
    tx.commit().await.unwrap();
    assert_eq!(blocked.await.unwrap(), Err(AuthError::Denied));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM browser_sessions")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::query("INSERT INTO principals (id,email,first_name,last_name,active) VALUES ('00000000-0000-0000-0000-000000000093','inactive@example.com','Inactive','User',false)").execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO credentials (id,principal_id,kind) VALUES ('00000000-0000-0000-0000-000000000193','00000000-0000-0000-0000-000000000093','password')").execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO password_credentials (credential_id,verifier) SELECT '00000000-0000-0000-0000-000000000193',verifier FROM password_credentials LIMIT 1").execute(&db.pool).await.unwrap();
    let inactive = db
        .store
        .candidate("inactive@example.com")
        .await
        .unwrap()
        .unwrap();
    assert!(!inactive.active);
    assert_eq!(
        db.store.establish(&inactive, [9; 32], None).await,
        Err(AuthError::Denied)
    );
    db.store.close().await;
    assert_eq!(db.store.session([1; 32]).await, Err(AuthError::Unavailable));
    assert_eq!(db.store.logout([1; 32]).await, Err(AuthError::Unavailable));
}

#[tokio::test]
async fn replicas_cannot_silently_split_budgets_with_different_keys() {
    let db = Database::new().await;
    let (a, b) = tokio::join!(
        db.store.bind_login_key([1; 32]),
        db.store.bind_login_key([2; 32])
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let winner = if a.is_ok() { [1; 32] } else { [2; 32] };
    db.store.bind_login_key(winner).await.unwrap();
    assert!(
        sqlx::query("DELETE FROM login_budget_policy")
            .execute(&db.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE login_budget_policy SET key_digest=key_digest")
            .execute(&db.pool)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn sessions_rotate_expire_logout_and_observe_committed_revocation() {
    let db = Database::new().await;
    let principal = db
        .store
        .bootstrap(administrator(90, "session@example.com"))
        .await
        .unwrap();
    assert!(
        db.store
            .candidate("unknown@example.com")
            .await
            .unwrap()
            .is_none()
    );
    let candidate = db
        .store
        .candidate("session@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [1; 32], None).await.unwrap();
    // A random-handle collision cannot revoke the old session on rollback.
    assert_eq!(
        db.store.establish(&candidate, [1; 32], Some([1; 32])).await,
        Err(AuthError::Unavailable)
    );
    assert_eq!(
        db.store.session([1; 32]).await.unwrap().principal,
        principal
    );
    let (a, b) = tokio::join!(
        db.store.establish(&candidate, [2; 32], Some([1; 32])),
        db.store.establish(&candidate, [3; 32], Some([1; 32]))
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(db.store.session([1; 32]).await, Err(AuthError::Denied));
    let winner = if a.is_ok() { [2; 32] } else { [3; 32] };
    db.store.logout(winner).await.unwrap();
    db.store.logout(winner).await.unwrap();
    assert_eq!(db.store.session(winner).await, Err(AuthError::Denied));
    db.store.establish(&candidate, [4; 32], None).await.unwrap();
    db.store
        .change(principal, 0, AccountAction::RevokeAll)
        .await
        .unwrap();
    assert_eq!(db.store.session([4; 32]).await, Err(AuthError::Denied));
    assert_eq!(
        db.store.establish(&candidate, [5; 32], None).await,
        Err(AuthError::Denied)
    );
    let current = db.store.candidate("session@example.com").await.unwrap();
    let current = current.unwrap();
    assert_eq!(current.epoch, 1);
    db.store.establish(&current, [6; 32], None).await.unwrap();
}

#[tokio::test]
async fn expired_sessions_clock_rollback_and_changed_verifiers_deny() {
    let db = Database::new().await;
    db.store
        .bootstrap(administrator(91, "expiry@example.com"))
        .await
        .unwrap();
    let mut candidate = db
        .store
        .candidate("expiry@example.com")
        .await
        .unwrap()
        .unwrap();
    for (digest, query) in [
        (
            [1; 32],
            "UPDATE browser_sessions SET seen_ms = seen_ms - 900001, created_ms = created_ms - 900001, expires_ms = expires_ms - 900001",
        ),
        (
            [2; 32],
            "UPDATE browser_sessions SET expires_ms = created_ms",
        ),
        (
            [3; 32],
            "UPDATE browser_sessions SET seen_ms = seen_ms + 5000",
        ),
    ] {
        db.store.establish(&candidate, digest, None).await.unwrap();
        // Controlled time fixture in this disposable database only.
        sqlx::query("ALTER TABLE browser_sessions DISABLE TRIGGER browser_session_transition")
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query(query).execute(&db.pool).await.unwrap();
        sqlx::query("ALTER TABLE browser_sessions ENABLE TRIGGER browser_session_transition")
            .execute(&db.pool)
            .await
            .unwrap();
        assert_eq!(db.store.session(digest).await, Err(AuthError::Denied));
    }
    candidate.verifier.push('a');
    assert_eq!(
        db.store.establish(&candidate, [9; 32], None).await,
        Err(AuthError::Denied)
    );
    assert_eq!(db.store.session([9; 32]).await, Err(AuthError::Denied));
}
