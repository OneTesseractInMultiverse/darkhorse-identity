use super::*;
use darkhorse_application::{authentication::AuthenticationStore, sessions::SessionManagement};
use darkhorse_domain::{
    identity::SessionId,
    sessions::{Error, Status},
};
async fn extra(db: &Database, digest: [u8; 32]) {
    let c = db
        .store
        .candidate("one@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&c, digest, None).await.unwrap();
}
async fn reference(db: &Database, digest: [u8; 32]) -> SessionId {
    SessionId::from_u128(
        sqlx::query_scalar::<_, Uuid>("SELECT public_id FROM browser_sessions WHERE digest=$1")
            .bind(digest.as_slice())
            .fetch_one(&db.pool)
            .await
            .unwrap()
            .as_u128(),
    )
    .unwrap()
}
async fn events(db: &Database, event: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM session_audit WHERE event=$1")
        .bind(event)
        .fetch_one(&db.pool)
        .await
        .unwrap()
}
#[tokio::test]
async fn session_history_is_owner_scoped_paginated_and_never_touches_idle_time() {
    let db = super::oidc::fixture().await;
    for n in 2..=30 {
        extra(&db, [n; 32]).await;
    }
    super::insert_principal(&db, 2, false).await;
    super::insert_password(&db, 2).await;
    let other = db
        .store
        .candidate("person2@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&other, [99; 32], None).await.unwrap();
    let seen: i64 = sqlx::query_scalar("SELECT seen_ms FROM browser_sessions WHERE digest=$1")
        .bind([1u8; 32].as_slice())
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let first = db.store.sessions([1; 32], None).await.unwrap();
    assert_eq!(first.items.len(), 25);
    assert_eq!(first.current, reference(&db, [1; 32]).await);
    let last = db.store.sessions([1; 32], first.next).await.unwrap();
    assert_eq!(last.items.len(), 5);
    assert!(last.next.is_none());
    let all = first
        .items
        .into_iter()
        .chain(last.items)
        .map(|r| r.id)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(all.len(), 30);
    assert!(!all.contains(&reference(&db, [99; 32]).await));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT seen_ms FROM browser_sessions WHERE digest=$1")
            .bind([1u8; 32].as_slice())
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        seen
    );
    assert_eq!(
        db.store
            .end_session([1; 32], reference(&db, [99; 32]).await)
            .await,
        Err(Error::NotFound)
    );
    assert_eq!(
        db.store
            .end_session([1; 32], SessionId::from_u128(999).unwrap())
            .await,
        Err(Error::NotFound)
    );
    assert_eq!(
        db.store.sessions([0; 32], None).await,
        Err(Error::Unauthorized)
    );
    assert_eq!(events(&db, "created").await, 31);
    db.store.close().await;
}
#[tokio::test]
async fn session_termination_is_atomic_idempotent_and_blocks_resurrection() {
    let db = super::oidc::fixture().await;
    extra(&db, [2; 32]).await;
    let target = reference(&db, [2; 32]).await;
    let (left, right) = tokio::join!(
        db.store.end_session([1; 32], target),
        db.store.end_session([1; 32], target)
    );
    assert!(!left.unwrap().current);
    assert!(!right.unwrap().current);
    assert_eq!(events(&db, "session_ended").await, 1);
    assert!(db.store.session([2; 32]).await.is_err());
    assert_eq!(
        db.store
            .sessions([1; 32], None)
            .await
            .unwrap()
            .items
            .iter()
            .find(|r| r.id == target)
            .unwrap()
            .status,
        Status::Inactive
    );
    for statement in [
        "UPDATE browser_sessions SET revoked=false WHERE revoked",
        "UPDATE browser_sessions SET public_id=gen_random_uuid()",
        "UPDATE browser_sessions SET credential_epoch=credential_epoch+1",
        "UPDATE browser_sessions SET seen_ms=seen_ms-1",
        "DELETE FROM session_audit",
        "UPDATE session_audit SET event='created'",
    ] {
        assert!(sqlx::query(statement).execute(&db.pool).await.is_err());
    }
    let own = reference(&db, [1; 32]).await;
    assert!(db.store.end_session([1; 32], own).await.unwrap().current);
    assert_eq!(
        db.store.sessions([1; 32], None).await,
        Err(Error::Unauthorized)
    );
    db.store.close().await;
}
#[tokio::test]
async fn session_audit_failure_rolls_back_login_replacement_and_all_termination_paths() {
    let db = super::oidc::fixture().await;
    extra(&db, [2; 32]).await;
    sqlx::raw_sql("CREATE FUNCTION reject_session_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected'; END $$; CREATE TRIGGER reject_session_audit BEFORE INSERT ON session_audit FOR EACH ROW EXECUTE FUNCTION reject_session_audit();").execute(&db.pool).await.unwrap();
    assert_eq!(
        db.store
            .end_session([1; 32], reference(&db, [2; 32]).await)
            .await,
        Err(Error::Unavailable)
    );
    assert!(db.store.logout([2; 32]).await.is_err());
    assert!(db.store.session([2; 32]).await.is_ok());
    let candidate = db
        .store
        .candidate("one@example.com")
        .await
        .unwrap()
        .unwrap();
    assert!(db.store.establish(&candidate, [3; 32], None).await.is_err());
    assert!(
        db.store
            .establish(&candidate, [4; 32], Some([1; 32]))
            .await
            .is_err()
    );
    assert!(db.store.session([1; 32]).await.is_ok());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM browser_sessions")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        2
    );
    db.store.close().await;
}
#[tokio::test]
async fn normal_sign_out_and_replacement_have_bounded_audit_without_recording_secret_material() {
    let db = super::oidc::fixture().await;
    let c = db
        .store
        .candidate("one@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store
        .establish(&c, [2; 32], Some([1; 32]))
        .await
        .unwrap();
    assert_eq!(events(&db, "replaced").await, 1);
    assert_eq!(events(&db, "created").await, 2);
    db.store.logout([2; 32]).await.unwrap();
    db.store.logout([2; 32]).await.unwrap();
    db.store.logout([0; 32]).await.unwrap();
    assert_eq!(events(&db, "signed_out").await, 1);
    let audit: Vec<serde_json::Value> =
        sqlx::query_scalar("SELECT to_jsonb(a) FROM session_audit a")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    for row in audit {
        let fields = row.as_object().unwrap();
        assert_eq!(fields.len(), 6);
        assert!(!fields.contains_key("digest"));
        assert!(!fields.contains_key("credential_id"));
    }
    db.store.close().await;
}
#[tokio::test]
async fn ending_one_session_denies_its_access_and_refresh_but_preserves_other_sessions() {
    use darkhorse_adapters::tokens::{
        material::{self, Purpose},
        signer::Signer,
    };
    use darkhorse_application::{
        refresh::{RefreshStore, Request},
        tokens::TokenStore,
    };
    let (db, signer): (Database, Signer) = super::tokens::setup().await;
    extra(&db, [2; 32]).await;
    sqlx::query("UPDATE oauth_clients SET refresh_tokens=true,revision=revision+1")
        .execute(&db.pool)
        .await
        .unwrap();
    let code = super::tokens::code(&db, [3; 32]).await;
    let token = db
        .store
        .redeem(
            super::tokens::input(&code),
            material::pair().unwrap(),
            super::tokens::ISSUER,
            &signer,
        )
        .await
        .unwrap();
    let target = reference(&db, [1; 32]).await;
    db.store.end_session([2; 32], target).await.unwrap();
    assert!(
        db.store
            .userinfo(
                material::digest(&token.access, Purpose::Access).unwrap(),
                super::tokens::ISSUER
            )
            .await
            .is_err()
    );
    assert!(
        db.store
            .refresh(
                Request {
                    client: super::oidc::request().client,
                    secret: [9; 32],
                    digest: material::digest(token.refresh.as_ref().unwrap(), Purpose::Refresh)
                        .unwrap(),
                    scopes: None,
                    resource: None
                },
                material::pair().unwrap(),
                super::tokens::ISSUER
            )
            .await
            .is_err()
    );
    assert!(db.store.session([2; 32]).await.is_ok());
    db.store.close().await;
}
#[tokio::test]
async fn repeated_and_unknown_termination_do_not_take_exclusive_security_locks() {
    let db = super::oidc::fixture().await;
    extra(&db, [2; 32]).await;
    let target = reference(&db, [2; 32]).await;
    db.store.end_session([1; 32], target).await.unwrap();
    let mut reader = db.pool.begin().await.unwrap();
    sqlx::query("SELECT singleton FROM security_state FOR SHARE")
        .execute(&mut *reader)
        .await
        .unwrap();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        db.store.end_session([1; 32], target),
    )
    .await;
    assert_eq!(
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            db.store
                .end_session([1; 32], SessionId::from_u128(999).unwrap())
        )
        .await
        .expect("unknown target blocked on an exclusive fence"),
        Err(Error::NotFound)
    );
    reader.rollback().await.unwrap();
    assert!(
        result
            .expect("read-only repeat blocked on an exclusive fence")
            .is_ok()
    );
    db.store.close().await;
}

async fn waiting(db: &Database, pattern: &str) {
    for _ in 0..200 {
        let found = sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE $1)")
            .bind(pattern).fetch_one(&db.pool).await.unwrap();
        if found {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("session operation did not reach its expected lock");
}
#[tokio::test]
async fn actor_revocation_after_preflight_cannot_authorize_a_session_change() {
    let db = super::oidc::fixture().await;
    extra(&db, [2; 32]).await;
    let target = reference(&db, [2; 32]).await;
    let mut blocker = db.pool.begin().await.unwrap();
    sqlx::query("SELECT singleton FROM security_state FOR SHARE")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let store = db.store.clone();
    let operation = tokio::spawn(async move { store.end_session([1; 32], target).await });
    waiting(&db, "SELECT singleton FROM security_state%FOR UPDATE").await;
    // Commit an actor revocation between the read-only preflight and final fence.
    sqlx::query("UPDATE browser_sessions SET revoked=true WHERE digest=$1")
        .bind([1u8; 32].as_slice())
        .execute(&mut *blocker)
        .await
        .unwrap();
    blocker.commit().await.unwrap();
    assert_eq!(operation.await.unwrap(), Err(Error::Unauthorized));
    assert!(db.store.session([2; 32]).await.is_ok());
    assert_eq!(events(&db, "session_ended").await, 0);
    db.store.close().await;
}
#[tokio::test]
async fn two_sessions_ending_each_other_have_one_authenticated_winner() {
    let db = super::oidc::fixture().await;
    extra(&db, [2; 32]).await;
    let first = reference(&db, [1; 32]).await;
    let second = reference(&db, [2; 32]).await;
    let (a, b) = tokio::join!(
        db.store.end_session([1; 32], second),
        db.store.end_session([2; 32], first)
    );
    assert!(matches!(
        (a, b),
        (Ok(_), Err(Error::Unauthorized)) | (Err(Error::Unauthorized), Ok(_))
    ));
    assert_eq!(events(&db, "session_ended").await, 1);
    db.store.close().await;
}
#[tokio::test]
async fn session_checks_and_termination_recheck_idle_expiry_after_row_waits() {
    use darkhorse_application::authentication::AuthError;
    for manage in [false, true] {
        let db = super::oidc::fixture().await;
        extra(&db, [2; 32]).await;
        let target = reference(&db, [2; 32]).await;
        // Only this disposable database moves immutable creation time to test expiry.
        sqlx::raw_sql("ALTER TABLE browser_sessions DISABLE TRIGGER browser_session_transition; UPDATE browser_sessions SET created_ms=created_ms-900000,expires_ms=expires_ms-900000,seen_ms=floor(extract(epoch FROM clock_timestamp())*1000)::bigint-898500 WHERE digest=decode(repeat('01',32),'hex'); ALTER TABLE browser_sessions ENABLE TRIGGER browser_session_transition;").execute(&db.pool).await.unwrap();
        let mut blocker = db.pool.begin().await.unwrap();
        sqlx::query("SELECT digest FROM browser_sessions WHERE digest=$1 FOR UPDATE")
            .bind([if manage { 2u8 } else { 1u8 }; 32].as_slice())
            .execute(&mut *blocker)
            .await
            .unwrap();
        let store = db.store.clone();
        let operation = tokio::spawn(async move {
            if manage {
                store.end_session([1; 32], target).await.map(|_| ())
            } else {
                store
                    .session([1; 32])
                    .await
                    .map(|_| ())
                    .map_err(|error| match error {
                        AuthError::Denied => Error::Unauthorized,
                        _ => Error::Unavailable,
                    })
            }
        });
        waiting(
            &db,
            if manage {
                "SELECT * FROM browser_sessions WHERE public_id=%FOR UPDATE"
            } else {
                "SELECT s.*, p.id%FOR UPDATE OF s"
            },
        )
        .await;
        sqlx::query("SELECT pg_sleep(GREATEST(0,(seen_ms+900000-floor(extract(epoch FROM clock_timestamp())*1000)::bigint)::double precision/1000)+0.02) FROM browser_sessions WHERE digest=decode(repeat('01',32),'hex')").execute(&db.pool).await.unwrap();
        blocker.rollback().await.unwrap();
        assert_eq!(operation.await.unwrap(), Err(Error::Unauthorized));
        assert_eq!(events(&db, "session_ended").await, 0);
        assert!(db.store.session([2; 32]).await.is_ok());
        db.store.close().await;
    }
}

#[tokio::test]
async fn session_upgrade_preserves_existing_handles_and_does_not_fabricate_audit_history() {
    let db = Database::at_version(12).await;
    db.store
        .bootstrap(administrator(1, "one@example.com"))
        .await
        .unwrap();
    sqlx::query("INSERT INTO browser_sessions(digest,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms) SELECT decode(repeat(n,32),'hex'),$1,$2,0,t,t,t+28800000 FROM (SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint t) clock CROSS JOIN (VALUES ('01'),('02')) v(n)").bind(Uuid::from_u128(1)).bind(Uuid::from_u128(101)).execute(&db.pool).await.unwrap();
    let before: Vec<serde_json::Value> =
        sqlx::query_scalar("SELECT to_jsonb(s) FROM browser_sessions s ORDER BY digest")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    db.store.migrate().await.unwrap();
    db.store.migrate().await.unwrap();
    let after: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT to_jsonb(s)-'public_id' FROM browser_sessions s ORDER BY digest",
    )
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(before, after);
    assert_ne!(reference(&db, [1; 32]).await, reference(&db, [2; 32]).await);
    assert_eq!(events(&db, "created").await, 0);
    assert!(db.store.session([1; 32]).await.is_ok());
    assert!(db.store.session([2; 32]).await.is_ok());
    extra(&db, [3; 32]).await;
    assert_eq!(events(&db, "created").await, 1);
    db.store.close().await;
}
