use super::*;
use darkhorse_application::{admin_directory::AdminDirectory, authentication::AuthenticationStore};
use darkhorse_domain::{
    admin_directory::{Change, Error, Names, Query},
    identity::{ApplicationId, RoleId},
};
fn query() -> Query {
    Query {
        status: None,
        search: String::new(),
        after: None,
        limit: 25,
    }
}
fn names() -> Change {
    Change::Names(Names::new("Grace", "Hopper").unwrap())
}
fn role(assigned: bool, policy_revision: u64) -> Change {
    Change::Role {
        application: ApplicationId::from_u128(16).unwrap(),
        role: RoleId::from_u128(48).unwrap(),
        assigned,
        policy_revision,
    }
}
async fn catalog(db: &Database) {
    sqlx::raw_sql("INSERT INTO roles(id,name) VALUES('00000000-0000-0000-0000-000000000030','Reader'); INSERT INTO role_applications(application_id,role_id) VALUES('00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000030');").execute(&db.pool).await.unwrap();
}
async fn audit_count(db: &Database) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM directory_admin_audit")
        .fetch_one(&db.pool)
        .await
        .unwrap()
}
#[tokio::test]
async fn directory_requires_live_administrator_and_recent_authentication_for_writes() {
    let db = oidc::fixture().await;
    insert_principal(&db, 2, false).await;
    insert_password(&db, 2).await;
    let candidate = db
        .store
        .candidate("person2@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [2; 32], None).await.unwrap();
    for (actor, error) in [([9; 32], Error::Unauthorized), ([2; 32], Error::Forbidden)] {
        assert!(matches!(db.store.users(actor,query()).await,Err(e) if e==error));
        assert!(matches!(db.store.user(actor,id(1)).await,Err(e) if e==error));
        assert!(matches!(db.store.access(actor,id(1),None).await,Err(e) if e==error));
        assert!(matches!(db.store.update(actor,id(2),0,names()).await,Err(e) if e==error));
    }
    sqlx::raw_sql("ALTER TABLE browser_sessions DISABLE TRIGGER browser_session_transition; UPDATE browser_sessions SET created_ms=created_ms-300000,expires_ms=expires_ms-300000 WHERE digest=decode(repeat('01',32),'hex'); ALTER TABLE browser_sessions ENABLE TRIGGER browser_session_transition;").execute(&db.pool).await.unwrap();
    assert!(db.store.users([1; 32], query()).await.is_ok());
    assert!(matches!(
        db.store.update([1; 32], id(2), 0, names()).await,
        Err(Error::RecentAuthentication)
    ));
    assert_eq!(audit_count(&db).await, 0);
    db.store.logout([1; 32]).await.unwrap();
    assert!(matches!(
        db.store.users([1; 32], query()).await,
        Err(Error::Unauthorized)
    ));
    db.store.close().await;
}
#[tokio::test]
async fn directory_keysets_pass_a_thousand_rows_and_search_treats_wildcards_literally() {
    let db = oidc::fixture().await;
    sqlx::query("INSERT INTO principals(id,email,first_name,last_name) SELECT lpad(to_hex(n),32,'0')::uuid,'person'||n||'@example.com','Test','Person' FROM generate_series(1000,2004) n").execute(&db.pool).await.unwrap();
    let mut q = query();
    q.limit = 100;
    let mut ids = std::collections::BTreeSet::new();
    loop {
        let page = db.store.users([1; 32], q.clone()).await.unwrap();
        assert!(page.items.len() <= 100);
        for row in page.items {
            assert!(ids.insert(row.id.as_u128()));
        }
        match page.next {
            Some(next) => q.after = Some(next),
            None => break,
        }
    }
    assert_eq!(ids.len(), 1006);
    for search in ["%", "_", "\\", "' OR true --"] {
        let mut q = query();
        q.search = search.into();
        assert!(db.store.users([1; 32], q).await.unwrap().items.is_empty());
    }
    let mut q = query();
    q.search = "LOVE".into();
    assert_eq!(db.store.users([1; 32], q).await.unwrap().items[0].id, id(1));
    let mut q = query();
    q.status = Some(AccountStatus::Inactive);
    assert!(db.store.users([1; 32], q).await.unwrap().items.is_empty());
    let mut q = query();
    q.limit = 101;
    assert!(matches!(
        db.store.users([1; 32], q).await,
        Err(Error::Invalid)
    ));
    assert!(matches!(
        db.store.user([1; 32], id(999)).await,
        Err(Error::NotFound)
    ));
    db.store.close().await;
}
#[tokio::test]
async fn directory_changes_preserve_credentials_and_require_fresh_revisions() {
    let db = oidc::fixture().await;
    insert_principal(&db, 2, false).await;
    insert_password(&db, 2).await;
    let candidate = db
        .store
        .candidate("person2@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [2; 32], None).await.unwrap();
    let row = db.store.update([1; 32], id(2), 0, names()).await.unwrap();
    assert_eq!(
        (
            row.first_name.as_str(),
            row.last_name.as_str(),
            row.email.as_str(),
            row.revision
        ),
        ("Grace", "Hopper", "person2@example.com", 1)
    );
    assert!(matches!(
        db.store.update([1; 32], id(2), 0, names()).await,
        Err(Error::Conflict)
    ));
    assert_eq!(
        db.store
            .update([1; 32], id(2), 1, names())
            .await
            .unwrap()
            .revision,
        1
    );
    assert_eq!(audit_count(&db).await, 1);
    let row = db
        .store
        .update([1; 32], id(2), 1, Change::Status(AccountStatus::Inactive))
        .await
        .unwrap();
    assert_eq!(row.revision, 2);
    assert_eq!(
        db.store.session([2; 32]).await,
        Err(darkhorse_application::authentication::AuthError::Denied)
    );
    let row = db
        .store
        .update([1; 32], id(2), 2, Change::Status(AccountStatus::Active))
        .await
        .unwrap();
    assert_eq!(row.revision, 3);
    assert_eq!(db.store.account(id(2)).await.unwrap().credential_epoch, 1);
    assert_eq!(
        db.store.session([2; 32]).await,
        Err(darkhorse_application::authentication::AuthError::Denied)
    );
    assert!(matches!(
        db.store
            .update([1; 32], id(1), 0, Change::Status(AccountStatus::Inactive))
            .await,
        Err(Error::LastAdministrator)
    ));
    assert_eq!(audit_count(&db).await, 3);
    assert!(
        sqlx::query("UPDATE directory_admin_audit SET event='reactivated'")
            .execute(&db.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM directory_admin_audit")
            .execute(&db.pool)
            .await
            .is_err()
    );
    db.store.close().await;
}
#[tokio::test]
async fn directory_role_assignments_bind_application_and_both_revisions() {
    let db = oidc::fixture().await;
    insert_principal(&db, 2, false).await;
    catalog(&db).await;
    let view = db.store.access([1; 32], id(2), None).await.unwrap();
    assert_eq!(view.selected, Some(ApplicationId::from_u128(16).unwrap()));
    assert_eq!(view.roles.len(), 1);
    assert!(!view.roles[0].assigned);
    assert!(matches!(
        db.store
            .access([1; 32], id(2), Some(ApplicationId::from_u128(99).unwrap()))
            .await,
        Err(Error::NotFound)
    ));
    let bad = Change::Role {
        application: ApplicationId::from_u128(99).unwrap(),
        role: RoleId::from_u128(48).unwrap(),
        assigned: true,
        policy_revision: view.policy_revision,
    };
    assert!(matches!(
        db.store.update([1; 32], id(2), 0, bad).await,
        Err(Error::Invalid)
    ));
    db.store
        .update([1; 32], id(2), 0, role(true, view.policy_revision))
        .await
        .unwrap();
    assert!(matches!(
        db.store
            .update([1; 32], id(2), 1, role(false, view.policy_revision))
            .await,
        Err(Error::Conflict)
    ));
    let view = db.store.access([1; 32], id(2), None).await.unwrap();
    assert!(view.roles[0].assigned);
    db.store
        .update([1; 32], id(2), 1, role(true, view.policy_revision))
        .await
        .unwrap();
    assert_eq!(
        db.store
            .access([1; 32], id(2), None)
            .await
            .unwrap()
            .policy_revision,
        view.policy_revision
    );
    sqlx::query("UPDATE applications SET active=false,revision=revision+1")
        .execute(&db.pool)
        .await
        .unwrap();
    let view = db.store.access([1; 32], id(2), None).await.unwrap();
    assert!(matches!(
        db.store
            .update([1; 32], id(2), 1, role(true, view.policy_revision))
            .await,
        Err(Error::Invalid)
    ));
    db.store
        .update([1; 32], id(2), 1, role(false, view.policy_revision))
        .await
        .unwrap();
    assert!(!db.store.access([1; 32], id(2), None).await.unwrap().roles[0].assigned);
    assert_eq!(audit_count(&db).await, 2);
    assert_eq!(db.store.account(id(2)).await.unwrap().credential_epoch, 0);
    db.store.close().await;
}
#[tokio::test]
async fn directory_audit_failure_rolls_back_every_mutation_and_policy_change() {
    let db = oidc::fixture().await;
    insert_principal(&db, 2, false).await;
    catalog(&db).await;
    let before = db.store.access([1; 32], id(2), None).await.unwrap();
    let audit = count(&db, "security_audit").await;
    sqlx::raw_sql("CREATE FUNCTION reject_directory_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected audit failure'; END $$; CREATE TRIGGER rejected_audit BEFORE INSERT ON directory_admin_audit FOR EACH ROW EXECUTE FUNCTION reject_directory_audit();").execute(&db.pool).await.unwrap();
    for change in [
        names(),
        Change::Status(AccountStatus::Inactive),
        role(true, before.policy_revision),
    ] {
        assert!(matches!(
            db.store.update([1; 32], id(2), 0, change).await,
            Err(Error::Unavailable)
        ));
        let after = db.store.access([1; 32], id(2), None).await.unwrap();
        assert_eq!(after.policy_revision, before.policy_revision);
        assert_eq!(after.user.revision, 0);
        assert_eq!(after.user.first_name, "Test");
        assert_eq!(after.user.status, AccountStatus::Active);
        assert!(!after.roles[0].assigned);
        assert_eq!(db.store.account(id(2)).await.unwrap().credential_epoch, 0);
        assert_eq!(count(&db, "security_audit").await, audit);
        assert_eq!(audit_count(&db).await, 0);
    }
    db.store.close().await;
}
#[tokio::test]
async fn directory_concurrent_admin_mutations_preserve_one_eligible_administrator() {
    let db = oidc::fixture().await;
    insert_principal(&db, 2, true).await;
    let candidate = db
        .store
        .candidate("person2@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [2; 32], None).await.unwrap();
    let (left, right) = tokio::join!(
        db.store
            .update([1; 32], id(1), 0, Change::Status(AccountStatus::Inactive)),
        db.store
            .update([2; 32], id(2), 0, Change::Status(AccountStatus::Inactive))
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    assert!(
        matches!(left, Err(Error::LastAdministrator))
            || matches!(right, Err(Error::LastAdministrator))
    );
    assert_eq!(audit_count(&db).await, 1);
    db.store.close().await;
}
#[tokio::test]
async fn directory_migration_preserves_existing_accounts() {
    let db = Database::at_version(16).await;
    db.store
        .bootstrap(administrator(1, "one@example.com"))
        .await
        .unwrap();
    db.store.migrate().await.unwrap();
    db.store.migrate().await.unwrap();
    assert_eq!(db.store.account(id(1)).await.unwrap().revision, 0);
    assert_eq!(audit_count(&db).await, 0);
    db.store.close().await;
}
async fn waiting(db: &Database, pattern: &str) {
    for _ in 0..200 {
        let blocked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND wait_event_type='Lock' AND query LIKE $1)").bind(pattern).fetch_one(&db.pool).await.unwrap();
        if blocked {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("directory operation did not reach expected lock");
}
#[tokio::test]
async fn directory_rechecks_committed_actor_revocation_after_waiting_for_authority() {
    let db = oidc::fixture().await;
    insert_principal(&db, 2, false).await;
    let mut blocker = db.pool.begin().await.unwrap();
    sqlx::query("SELECT singleton FROM security_state FOR SHARE")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let store = db.store.clone();
    let operation = tokio::spawn(async move { store.update([1; 32], id(2), 0, names()).await });
    waiting(&db, "SELECT singleton FROM security_state%FOR UPDATE").await;
    sqlx::query(
        "UPDATE browser_sessions SET revoked=true WHERE digest=decode(repeat('01',32),'hex')",
    )
    .execute(&mut *blocker)
    .await
    .unwrap();
    blocker.commit().await.unwrap();
    assert!(matches!(operation.await.unwrap(), Err(Error::Unauthorized)));
    assert_eq!(db.store.account(id(2)).await.unwrap().revision, 0);
    assert_eq!(audit_count(&db).await, 0);
    db.store.close().await;
}
#[tokio::test]
async fn directory_rechecks_recent_authentication_after_waiting_for_target() {
    let db = oidc::fixture().await;
    insert_principal(&db, 2, false).await;
    sqlx::raw_sql("ALTER TABLE browser_sessions DISABLE TRIGGER browser_session_transition; UPDATE browser_sessions SET created_ms=created_ms-298500,expires_ms=expires_ms-298500; ALTER TABLE browser_sessions ENABLE TRIGGER browser_session_transition;").execute(&db.pool).await.unwrap();
    let mut blocker = db.pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM principals WHERE id=$1 FOR UPDATE")
        .bind(Uuid::from_u128(2))
        .execute(&mut *blocker)
        .await
        .unwrap();
    let store = db.store.clone();
    let operation = tokio::spawn(async move { store.update([1; 32], id(2), 0, names()).await });
    waiting(&db, "SELECT p.*%FOR UPDATE%").await;
    sqlx::query("SELECT pg_sleep(GREATEST(0,(created_ms+300000-floor(extract(epoch FROM clock_timestamp())*1000)::bigint)::double precision/1000)+0.02) FROM browser_sessions").execute(&db.pool).await.unwrap();
    blocker.rollback().await.unwrap();
    assert!(matches!(
        operation.await.unwrap(),
        Err(Error::RecentAuthentication)
    ));
    assert_eq!(db.store.account(id(2)).await.unwrap().revision, 0);
    assert_eq!(audit_count(&db).await, 0);
    db.store.close().await;
}
#[tokio::test]
async fn directory_access_catalogs_reject_overflow_without_partial_permissions() {
    let db = oidc::fixture().await;
    catalog(&db).await;
    sqlx::query("INSERT INTO roles(id,name) SELECT lpad(to_hex(n),32,'0')::uuid,'Role '||n FROM generate_series(1000,1127) n").execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO role_applications(application_id,role_id) SELECT '00000000-0000-0000-0000-000000000010',lpad(to_hex(n),32,'0')::uuid FROM generate_series(1000,1127) n").execute(&db.pool).await.unwrap();
    assert!(matches!(
        db.store.access([1; 32], id(1), None).await,
        Err(Error::Unavailable)
    ));
    sqlx::query(
        "DELETE FROM role_applications WHERE role_id='00000000-0000-0000-0000-000000000030'",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        db.store
            .access([1; 32], id(1), None)
            .await
            .unwrap()
            .roles
            .len(),
        128
    );
    sqlx::query("INSERT INTO applications(id,name,owner_id,active) SELECT lpad(to_hex(n),32,'0')::uuid,'App '||n,'00000000-0000-0000-0000-000000000001',true FROM generate_series(1000,1099) n").execute(&db.pool).await.unwrap();
    assert!(matches!(
        db.store.access([1; 32], id(1), None).await,
        Err(Error::Unavailable)
    ));
    db.store.close().await;
}
