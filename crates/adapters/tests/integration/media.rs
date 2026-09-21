use super::*;
use darkhorse_application::{
    authentication::AuthenticationStore,
    media::{Prepared, Store, Target, Ticket},
};
use darkhorse_domain::{media::Kind, profiles::Error};
fn portrait() -> Target {
    Target {
        kind: Kind::Portrait,
        principal: None,
    }
}
fn logo() -> Target {
    Target {
        kind: Kind::Logo,
        principal: None,
    }
}
fn prepared() -> Prepared {
    Prepared {
        bytes: vec![1, 2, 3],
        digest: [4; 32],
        width: 1,
        height: 1,
    }
}
async fn user(db: &Database) {
    insert_principal(db, 2, false).await;
    insert_password(db, 2).await;
    let candidate = db
        .store
        .candidate("person2@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [2; 32], None).await.unwrap();
}
#[tokio::test]
async fn media_publication_is_revision_checked_audited_and_removal_is_immediate() {
    let db = oidc::fixture().await;
    user(&db).await;
    assert!(
        db.store
            .asset(Some([2; 32]), portrait())
            .await
            .unwrap()
            .is_none()
    );
    let ticket = db.store.reserve([2; 32], portrait(), 0).await.unwrap();
    assert!(
        db.store
            .asset(Some([2; 32]), portrait())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        db.store
            .attach([2; 32], ticket.clone(), 0, &prepared())
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        db.store
            .asset(Some([2; 32]), portrait())
            .await
            .unwrap()
            .unwrap()
            .id,
        ticket.id
    );
    assert!(matches!(
        db.store.asset(None, portrait()).await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        db.store
            .asset(
                Some([2; 32]),
                Target {
                    kind: Kind::Portrait,
                    principal: Some(id(1))
                }
            )
            .await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        db.store.attach([2; 32], ticket, 1, &prepared()).await,
        Err(Error::Conflict)
    ));
    assert_eq!(db.store.remove([2; 32], portrait(), 1).await.unwrap(), 2);
    assert!(
        db.store
            .asset(Some([2; 32]), portrait())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(db.store.remove([2; 32], portrait(), 2).await.unwrap(), 2);
    let tombstones = db.store.garbage().await.unwrap();
    assert_eq!(tombstones.len(), 1);
    db.store.cleaned(tombstones[0]).await.unwrap();
    assert!(db.store.garbage().await.unwrap().is_empty());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM media_audit")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        2
    );
    assert!(
        sqlx::query("DELETE FROM media_audit")
            .execute(&db.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE media_assets SET state='ready'")
            .execute(&db.pool)
            .await
            .is_err()
    );
    db.store.close().await;
}
#[tokio::test]
async fn branding_is_publicly_readable_but_only_recent_administrators_can_change_it() {
    let db = oidc::fixture().await;
    user(&db).await;
    assert!(matches!(
        db.store.reserve([2; 32], logo(), 0).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        db.store.branding([2; 32]).await,
        Err(Error::Forbidden)
    ));
    let ticket = db.store.reserve([1; 32], logo(), 0).await.unwrap();
    db.store
        .attach([1; 32], ticket, 0, &prepared())
        .await
        .unwrap();
    assert!(db.store.asset(None, logo()).await.unwrap().is_some());
    let branding = db.store.branding([1; 32]).await.unwrap();
    assert_eq!(branding.revision, 1);
    assert!(branding.logo);
    assert!(!branding.background);
    assert!(matches!(
        db.store
            .asset(
                None,
                Target {
                    kind: Kind::Logo,
                    principal: Some(id(1))
                }
            )
            .await,
        Err(Error::Invalid)
    ));
    assert_eq!(db.store.remove([1; 32], logo(), 1).await.unwrap(), 2);
    assert!(db.store.asset(None, logo()).await.unwrap().is_none());
    db.store.close().await;
}
#[tokio::test]
async fn interrupted_or_racing_uploads_stay_unpublished_and_become_cleanup_tombstones() {
    let db = oidc::fixture().await;
    user(&db).await;
    let first = db.store.reserve([2; 32], portrait(), 0).await.unwrap();
    let second = db.store.reserve([2; 32], portrait(), 0).await.unwrap();
    db.store
        .attach([2; 32], first, 0, &prepared())
        .await
        .unwrap();
    assert!(matches!(
        db.store.attach([2; 32], second, 0, &prepared()).await,
        Err(Error::Conflict)
    ));
    sqlx::raw_sql("ALTER TABLE media_assets DISABLE TRIGGER media_assets_transition; UPDATE media_assets SET created_ms=created_ms-4000000,cleanup_after_ms=cleanup_after_ms-4000000 WHERE state='pending'; SET CONSTRAINTS ALL IMMEDIATE; ALTER TABLE media_assets ENABLE TRIGGER media_assets_transition;").execute(&db.pool).await.unwrap();
    assert_eq!(db.store.garbage().await.unwrap().len(), 1);
    assert!(
        db.store
            .asset(Some([2; 32]), portrait())
            .await
            .unwrap()
            .is_some()
    );
    db.store.close().await;
}
#[tokio::test]
async fn failed_audit_or_lost_authority_cannot_publish_uploaded_bytes() {
    let db = oidc::fixture().await;
    user(&db).await;
    let ticket = db.store.reserve([2; 32], portrait(), 0).await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION reject_media_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test'; END $$; CREATE TRIGGER reject_media BEFORE INSERT ON media_audit FOR EACH ROW EXECUTE FUNCTION reject_media_audit();").execute(&db.pool).await.unwrap();
    assert!(matches!(
        db.store
            .attach([2; 32], ticket.clone(), 0, &prepared())
            .await,
        Err(Error::Unavailable)
    ));
    assert!(
        db.store
            .asset(Some([2; 32]), portrait())
            .await
            .unwrap()
            .is_none()
    );
    sqlx::query("DROP TRIGGER reject_media ON media_audit")
        .execute(&db.pool)
        .await
        .unwrap();
    db.store.logout([2; 32]).await.unwrap();
    assert!(matches!(
        db.store.attach([2; 32], ticket, 0, &prepared()).await,
        Err(Error::Unauthorized)
    ));
    db.store.close().await;
}
#[tokio::test]
async fn upload_budget_and_storage_binding_are_durable() {
    let db = oidc::fixture().await;
    db.store.bind_object_storage([1; 32]).await.unwrap();
    db.store.bind_object_storage([1; 32]).await.unwrap();
    assert_eq!(
        db.store.bind_object_storage([2; 32]).await,
        Err(Error::Conflict)
    );
    for _ in 0..4 {
        db.store.reserve([1; 32], portrait(), 0).await.unwrap();
    }
    assert!(matches!(
        db.store.reserve([1; 32], portrait(), 0).await,
        Err(Error::Invalid)
    ));
    let id = darkhorse_domain::identity::AssetId::from_u128(900).unwrap();
    assert!(matches!(
        db.store
            .attach(
                [1; 32],
                Ticket {
                    id,
                    target: Target {
                        principal: Some(super::id(1)),
                        ..portrait()
                    }
                },
                0,
                &prepared()
            )
            .await,
        Err(Error::NotFound)
    ));
    db.store.close().await;
}

#[tokio::test]
async fn media_writes_recheck_recent_authentication_after_waiting_for_the_target() {
    for operation in ["reserve", "attach", "remove"] {
        let db = oidc::fixture().await;
        insert_principal(&db, 2, false).await;
        let target = Target {
            kind: Kind::Portrait,
            principal: Some(id(2)),
        };
        let ticket = db.store.reserve([1; 32], target, 0).await.unwrap();
        sqlx::raw_sql("ALTER TABLE browser_sessions DISABLE TRIGGER browser_session_transition; UPDATE browser_sessions SET created_ms=created_ms-298500,expires_ms=expires_ms-298500; ALTER TABLE browser_sessions ENABLE TRIGGER browser_session_transition;").execute(&db.pool).await.unwrap();
        let mut blocker = db.pool.begin().await.unwrap();
        sqlx::query("SELECT id FROM principals WHERE id=$1 FOR UPDATE")
            .bind(Uuid::from_u128(2))
            .execute(&mut *blocker)
            .await
            .unwrap();
        let store = db.store.clone();
        let task = tokio::spawn(async move {
            match operation {
                "reserve" => store.reserve([1; 32], target, 0).await.map(|_| 0),
                "attach" => store.attach([1; 32], ticket, 0, &prepared()).await,
                _ => store.remove([1; 32], target, 0).await,
            }
        });
        tokio::time::timeout(std::time::Duration::from_secs(2),async {loop {let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'SELECT revision,portrait_id%')").fetch_one(&db.pool).await.unwrap();if waiting{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();
        sqlx::query("SELECT pg_sleep(GREATEST(0,(created_ms+300000-floor(extract(epoch FROM clock_timestamp())*1000)::bigint)::double precision/1000)+0.02) FROM browser_sessions").execute(&db.pool).await.unwrap();
        blocker.rollback().await.unwrap();
        assert_eq!(
            task.await.unwrap(),
            Err(Error::RecentAuthentication),
            "{operation}"
        );
        db.store.close().await;
    }
}
