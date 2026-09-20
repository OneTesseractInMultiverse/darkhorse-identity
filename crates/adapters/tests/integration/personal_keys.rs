use super::*;
use darkhorse_adapters::personal_keys::{OsKeyEntropy, digest};
use darkhorse_application::personal_keys::{Introspection, Probe, Service, Store};
use darkhorse_domain::{
    authorization::CapabilitySelection,
    identity::*,
    personal_keys::{Error, Expiration, Request, Selection},
};
fn resource() -> ResourceId {
    ResourceId::from_u128(0x30).unwrap()
}
async fn request(db: &Database, selection: CapabilitySelection) -> Request {
    let options = db.store.options([1; 32], None).await.unwrap();
    assert_eq!(options.items.len(), 1);
    Request::new(
        "deployment worker",
        ApplicationId::from_u128(0x10).unwrap(),
        options.revision,
        Expiration::Never,
        vec![Selection {
            resource: resource(),
            capabilities: selection,
        }],
    )
    .unwrap()
}
#[tokio::test]
async fn personal_keys_snapshot_own_rights_and_terminal_revocation_across_connections() {
    let (db, _) = super::tokens::setup().await;
    super::resource_tokens::policy(&db).await;
    let registry = darkhorse_application::resource_servers::Service {
        store: db.store.clone(),
        entropy: darkhorse_adapters::resource_servers::OsResourceEntropy,
    };
    use darkhorse_application::resource_servers::{Change, Command, Registry, Target};
    let registered = registry
        .write(
            [1; 32],
            Command {
                target: Target {
                    application: ApplicationId::from_u128(0x10).unwrap(),
                    resource: resource(),
                },
                change: Change::Register,
            },
        )
        .await
        .unwrap();
    let secret =
        darkhorse_adapters::resource_servers::secret_digest(&registered.secret.unwrap()).unwrap();
    let service = Service {
        store: db.store.clone(),
        entropy: OsKeyEntropy,
    };
    let input = request(
        &db,
        CapabilitySelection::Subset([CapabilityId::from_u128(0x70).unwrap()].into()),
    )
    .await;
    let key = service.create([1; 32], &input).await.unwrap();
    let probe = || Probe {
        resource: resource(),
        secret,
        key: Some(digest(&key.secret).unwrap()),
    };
    let replica = PostgresStore::from_pool(db.pool.clone());
    let active = replica
        .introspect_key(probe(), super::tokens::ISSUER)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(active.capabilities.len(), 1);
    assert_eq!(active.expires, None);
    sqlx::query(
        "DELETE FROM role_capabilities WHERE capability_id='00000000-0000-0000-0000-000000000070'",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    assert!(
        replica
            .introspect_key(probe(), super::tokens::ISSUER)
            .await
            .unwrap()
            .is_none()
    );
    sqlx::query("INSERT INTO role_capabilities VALUES('00000000-0000-0000-0000-000000000080','00000000-0000-0000-0000-000000000070')").execute(&db.pool).await.unwrap();
    assert_eq!(
        replica
            .introspect_key(probe(), super::tokens::ISSUER)
            .await
            .unwrap()
            .unwrap()
            .capabilities
            .len(),
        1
    );
    Store::revoke(&db.store, [1; 32], key.record.id)
        .await
        .unwrap();
    Store::revoke(&db.store, [1; 32], key.record.id)
        .await
        .unwrap();
    assert!(
        replica
            .introspect_key(probe(), super::tokens::ISSUER)
            .await
            .unwrap()
            .is_none()
    );
    assert!(!Store::list(&db.store, [1; 32], None).await.unwrap().items[0].active);
    let events: i64 = sqlx::query_scalar("SELECT count(*) FROM personal_key_audit")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(events, 2);
    assert_eq!(
        service.create([0; 32], &input).await.err(),
        Some(Error::Unauthorized)
    );
    db.store.close().await;
}

async fn fixture() -> (Database, Service<PostgresStore, OsKeyEntropy>, [u8; 32]) {
    let (db, _) = super::tokens::setup().await;
    super::resource_tokens::policy(&db).await;
    use darkhorse_application::resource_servers::{Change, Command, Registry, Target};
    let registry = darkhorse_application::resource_servers::Service {
        store: db.store.clone(),
        entropy: darkhorse_adapters::resource_servers::OsResourceEntropy,
    };
    let registered = registry
        .write(
            [1; 32],
            Command {
                target: Target {
                    application: ApplicationId::from_u128(0x10).unwrap(),
                    resource: resource(),
                },
                change: Change::Register,
            },
        )
        .await
        .unwrap();
    let secret =
        darkhorse_adapters::resource_servers::secret_digest(&registered.secret.unwrap()).unwrap();
    let service = Service {
        store: db.store.clone(),
        entropy: OsKeyEntropy,
    };
    (db, service, secret)
}
async fn total(db: &Database) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM personal_keys")
        .fetch_one(&db.pool)
        .await
        .unwrap()
}
async fn another_user(db: &Database) {
    use darkhorse_application::authentication::AuthenticationStore;
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
async fn personal_keys_issuance_is_owner_scoped_and_cannot_delegate_administrator_authority() {
    let (db, service, _) = fixture().await;
    another_user(&db).await;
    assert!(
        db.store
            .options([2; 32], None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    let input = request(&db, CapabilitySelection::All).await;
    assert_eq!(
        service.create([2; 32], &input).await.err(),
        Some(Error::Forbidden)
    );
    let excessive = request(
        &db,
        CapabilitySelection::Subset([CapabilityId::from_u128(999).unwrap()].into()),
    )
    .await;
    assert_eq!(
        service.create([1; 32], &excessive).await.err(),
        Some(Error::Forbidden)
    );
    let wrong_app = Request::new(
        "worker",
        ApplicationId::from_u128(999).unwrap(),
        input.revision(),
        Expiration::Never,
        input.grants().to_vec(),
    )
    .unwrap();
    assert_eq!(
        service.create([1; 32], &wrong_app).await.err(),
        Some(Error::Forbidden)
    );
    let key = service.create([1; 32], &input).await.unwrap();
    assert!(
        Store::list(&db.store, [2; 32], None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert_eq!(
        Store::revoke(&db.store, [2; 32], key.record.id).await,
        Err(Error::NotFound)
    );
    assert_eq!(
        Store::revoke(&db.store, [2; 32], CredentialId::from_u128(999).unwrap()).await,
        Err(Error::NotFound)
    );
    sqlx::query("DELETE FROM principal_roles")
        .execute(&db.pool)
        .await
        .unwrap();
    let revision: u64 = db.store.options([1; 32], None).await.unwrap().revision;
    let denied = Request::new(
        "admin",
        input.application(),
        revision,
        Expiration::Never,
        input.grants().to_vec(),
    )
    .unwrap();
    assert_eq!(
        service.create([1; 32], &denied).await.err(),
        Some(Error::Forbidden)
    );
    db.store.close().await;
}
#[tokio::test]
async fn personal_keys_audit_failure_and_immutable_storage_prevent_partial_or_expanded_credentials()
{
    let (db, service, _) = fixture().await;
    let input = request(&db, CapabilitySelection::All).await;
    sqlx::raw_sql("CREATE FUNCTION reject_personal_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected failure'; END $$; CREATE TRIGGER reject_personal_audit BEFORE INSERT ON personal_key_audit FOR EACH ROW EXECUTE FUNCTION reject_personal_audit();").execute(&db.pool).await.unwrap();
    assert_eq!(
        service.create([1; 32], &input).await.err(),
        Some(Error::Unavailable)
    );
    assert_eq!(total(&db).await, 0);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM credentials WHERE kind='personal_key'")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query("DROP TRIGGER reject_personal_audit ON personal_key_audit")
        .execute(&db.pool)
        .await
        .unwrap();
    let key = service.create([1; 32], &input).await.unwrap();
    for query in [
        "UPDATE personal_keys SET name='changed'",
        "DELETE FROM personal_keys",
        "UPDATE personal_key_verifiers SET verifier=decode(repeat('ab',32),'hex')",
        "DELETE FROM personal_key_grants",
        "UPDATE personal_key_grants SET capability_ceiling=ARRAY['00000000-0000-0000-0000-000000000071']::uuid[]",
        "DELETE FROM personal_key_audit",
        "INSERT INTO personal_key_grants SELECT * FROM personal_key_grants",
    ] {
        assert!(
            sqlx::query(query).execute(&db.pool).await.is_err(),
            "{query}"
        );
    }
    sqlx::query("CREATE TRIGGER reject_personal_audit BEFORE INSERT ON personal_key_audit FOR EACH ROW EXECUTE FUNCTION reject_personal_audit()").execute(&db.pool).await.unwrap();
    assert_eq!(
        Store::revoke(&db.store, [1; 32], key.record.id).await,
        Err(Error::Unavailable)
    );
    assert!(Store::list(&db.store, [1; 32], None).await.unwrap().items[0].active);
    let serialized: String =
        sqlx::query_scalar("SELECT row_to_json(v)::text FROM personal_key_verifiers v")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert!(!serialized.contains(&key.secret));
    db.store.close().await;
}
#[tokio::test]
async fn personal_keys_expiration_policy_recent_authentication_and_issuance_limits_are_enforced() {
    let (db, service, secret) = fixture().await;
    sqlx::query("UPDATE personal_key_policy SET allow_never=false")
        .execute(&db.pool)
        .await
        .unwrap();
    let input = request(&db, CapabilitySelection::All).await;
    assert_eq!(
        service.create([1; 32], &input).await.err(),
        Some(Error::Forbidden)
    );
    let expiring = Request::new(
        "timed",
        input.application(),
        input.revision(),
        Expiration::Default,
        input.grants().to_vec(),
    )
    .unwrap();
    let key = service.create([1; 32], &expiring).await.unwrap();
    assert_eq!(
        key.record.expires_ms.unwrap() - key.record.created_ms,
        30 * 86_400_000
    );
    sqlx::raw_sql("ALTER TABLE personal_keys DISABLE TRIGGER personal_key_immutable; UPDATE personal_keys SET created_ms=created_ms-2592000001,expires_ms=expires_ms-2592000001; ALTER TABLE personal_keys ENABLE TRIGGER personal_key_immutable;").execute(&db.pool).await.unwrap();
    assert!(
        db.store
            .introspect_key(
                Probe {
                    resource: resource(),
                    secret,
                    key: Some(digest(&key.secret).unwrap())
                },
                super::tokens::ISSUER
            )
            .await
            .unwrap()
            .is_none()
    );
    for _ in 0..10 {
        let input = request(&db, CapabilitySelection::All).await;
        let fresh = Request::new(
            "timed",
            input.application(),
            input.revision(),
            Expiration::Default,
            input.grants().to_vec(),
        )
        .unwrap();
        service.create([1; 32], &fresh).await.unwrap();
    }
    let input = request(&db, CapabilitySelection::All).await;
    let expiring = Request::new(
        "timed",
        input.application(),
        input.revision(),
        Expiration::Default,
        input.grants().to_vec(),
    )
    .unwrap();
    assert_eq!(
        service.create([1; 32], &expiring).await.err(),
        Some(Error::Limit)
    );
    sqlx::raw_sql("ALTER TABLE browser_sessions DISABLE TRIGGER browser_session_transition; UPDATE browser_sessions SET created_ms=created_ms-300001,expires_ms=expires_ms-300001; ALTER TABLE browser_sessions ENABLE TRIGGER browser_session_transition;").execute(&db.pool).await.unwrap();
    assert_eq!(
        Store::revoke(&db.store, [1; 32], key.record.id).await,
        Err(Error::RecentAuthenticationRequired)
    );
    assert_eq!(
        service.create([1; 32], &expiring).await.err(),
        Some(Error::RecentAuthenticationRequired)
    );
    assert_eq!(
        Store::list(&db.store, [1; 32], None)
            .await
            .unwrap()
            .items
            .len(),
        11
    );
    db.store.close().await;
}
#[tokio::test]
async fn personal_keys_account_epoch_and_resource_credentials_are_checked_live() {
    let (db, service, secret) = fixture().await;
    let key = service
        .create([1; 32], &request(&db, CapabilitySelection::All).await)
        .await
        .unwrap();
    let probe = || Probe {
        resource: resource(),
        secret,
        key: Some(digest(&key.secret).unwrap()),
    };
    assert!(
        db.store
            .introspect_key(probe(), "https://wrong.example")
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        db.store
            .introspect_key(
                Probe {
                    secret: [0; 32],
                    ..probe()
                },
                super::tokens::ISSUER
            )
            .await,
        Err(darkhorse_domain::tokens::Error::InvalidClient)
    ));
    assert!(
        db.store
            .introspect_key(
                Probe {
                    key: None,
                    ..probe()
                },
                super::tokens::ISSUER
            )
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        db.store
            .introspect_key(
                Probe {
                    key: Some([0; 32]),
                    ..probe()
                },
                super::tokens::ISSUER
            )
            .await
            .unwrap()
            .is_none()
    );
    // Application ownership is contact metadata; another administrator permits this lifecycle exercise.
    insert_principal(&db, 2, true).await;
    db.store
        .change(id(1), 0, AccountAction::SetStatus(AccountStatus::Inactive))
        .await
        .unwrap();
    assert!(
        db.store
            .introspect_key(probe(), super::tokens::ISSUER)
            .await
            .unwrap()
            .is_none()
    );
    db.store
        .change(id(1), 1, AccountAction::SetStatus(AccountStatus::Active))
        .await
        .unwrap();
    assert!(
        db.store
            .introspect_key(probe(), super::tokens::ISSUER)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        service
            .create([1; 32], &request_for_revision(0))
            .await
            .err(),
        Some(Error::Unauthorized)
    );
    db.store.close().await;
}
fn request_for_revision(revision: u64) -> Request {
    Request::new(
        "worker",
        ApplicationId::from_u128(0x10).unwrap(),
        revision,
        Expiration::Never,
        vec![Selection {
            resource: resource(),
            capabilities: CapabilitySelection::All,
        }],
    )
    .unwrap()
}

#[tokio::test]
async fn personal_keys_multiple_resource_ceilings_never_follow_future_grants() {
    let (db, service, secret) = fixture().await;
    sqlx::raw_sql("INSERT INTO protected_resources(id,application_id,name,audience) VALUES('00000000-0000-0000-0000-000000000031','00000000-0000-0000-0000-000000000010','Second API','urn:darkhorse:resource:00000000-0000-0000-0000-000000000031'); INSERT INTO resource_capabilities VALUES('00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000031','00000000-0000-0000-0000-000000000070');").execute(&db.pool).await.unwrap();
    use darkhorse_application::resource_servers::{Change, Command, Registry, Target};
    let registry = darkhorse_application::resource_servers::Service {
        store: db.store.clone(),
        entropy: darkhorse_adapters::resource_servers::OsResourceEntropy,
    };
    let other = ResourceId::from_u128(0x31).unwrap();
    let registered = registry
        .write(
            [1; 32],
            Command {
                target: Target {
                    application: ApplicationId::from_u128(0x10).unwrap(),
                    resource: other,
                },
                change: Change::Register,
            },
        )
        .await
        .unwrap();
    let other_secret =
        darkhorse_adapters::resource_servers::secret_digest(&registered.secret.unwrap()).unwrap();
    let options = db.store.options([1; 32], None).await.unwrap();
    assert_eq!(options.items.len(), 2);
    let input = Request::new(
        "multi",
        ApplicationId::from_u128(0x10).unwrap(),
        options.revision,
        Expiration::Never,
        vec![
            Selection {
                resource: resource(),
                capabilities: CapabilitySelection::All,
            },
            Selection {
                resource: other,
                capabilities: CapabilitySelection::All,
            },
        ],
    )
    .unwrap();
    let key = service.create([1; 32], &input).await.unwrap();
    assert_eq!(key.record.grants.len(), 2);
    sqlx::raw_sql("INSERT INTO capabilities(id,permission_key,meaning) VALUES('00000000-0000-0000-0000-000000000072','new','New permission'); INSERT INTO capability_applications VALUES('00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000072'); INSERT INTO role_capabilities VALUES('00000000-0000-0000-0000-000000000080','00000000-0000-0000-0000-000000000072'); INSERT INTO resource_capabilities VALUES('00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000030','00000000-0000-0000-0000-000000000072');").execute(&db.pool).await.unwrap();
    for (resource, secret, count) in [(resource(), secret, 2), (other, other_secret, 1)] {
        let active = db
            .store
            .introspect_key(
                Probe {
                    resource,
                    secret,
                    key: Some(digest(&key.secret).unwrap()),
                },
                super::tokens::ISSUER,
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(active.capabilities.len(), count);
        assert!(
            !active
                .capabilities
                .contains(&CapabilityId::from_u128(0x72).unwrap())
        );
    }
    // Revoking a personal credential does not terminate its creator's browser session.
    Store::revoke(&db.store, [1; 32], key.record.id)
        .await
        .unwrap();
    assert!(db.store.options([1; 32], None).await.is_ok());
    assert!(
        db.store
            .introspect_key(
                Probe {
                    resource: other,
                    secret: other_secret,
                    key: Some(digest(&key.secret).unwrap())
                },
                super::tokens::ISSUER
            )
            .await
            .unwrap()
            .is_none()
    );
    db.store.close().await;
}

#[tokio::test]
async fn personal_keys_paginate_without_duplicates_and_enforce_live_key_capacity() {
    let (db, service, _) = fixture().await;
    for n in 0..100 {
        if n % 10 == 0 {
            sqlx::raw_sql("ALTER TABLE personal_keys DISABLE TRIGGER personal_key_immutable; UPDATE personal_keys SET created_ms=created_ms-600001; ALTER TABLE personal_keys ENABLE TRIGGER personal_key_immutable;").execute(&db.pool).await.unwrap();
        }
        service
            .create([1; 32], &request(&db, CapabilitySelection::All).await)
            .await
            .unwrap();
    }
    let request = request(&db, CapabilitySelection::All).await;
    assert_eq!(
        service.create([1; 32], &request).await.err(),
        Some(Error::Limit)
    );
    let mut after = None;
    let mut found = std::collections::BTreeSet::new();
    loop {
        let page = Store::list(&db.store, [1; 32], after).await.unwrap();
        assert_eq!(page.items.len(), 25);
        for item in &page.items {
            assert!(item.active);
            assert!(found.insert(item.id));
            if let Some(previous) = after {
                assert!(item.id > previous);
            }
        }
        after = page.next;
        if after.is_none() {
            break;
        }
    }
    assert_eq!(found.len(), 100);
    assert!(
        Store::list(
            &db.store,
            [1; 32],
            Some(CredentialId::from_u128(u128::MAX).unwrap())
        )
        .await
        .unwrap()
        .items
        .is_empty()
    );
    db.store.close().await;
}

#[tokio::test]
async fn personal_keys_unavailable_or_oversize_authority_never_becomes_an_active_or_inactive_answer()
 {
    let (db, service, secret) = fixture().await;
    let key = service
        .create([1; 32], &request(&db, CapabilitySelection::All).await)
        .await
        .unwrap();
    let probe = || Probe {
        resource: resource(),
        secret,
        key: Some(digest(&key.secret).unwrap()),
    };
    // Additional empty assigned roles still count toward the projection bound.
    sqlx::raw_sql("INSERT INTO roles(id,name) SELECT md5('personal-test-role-'||n)::uuid,'Overflow '||n FROM generate_series(1,65) n; INSERT INTO role_applications SELECT '00000000-0000-0000-0000-000000000010',id FROM roles WHERE name LIKE 'Overflow %'; INSERT INTO principal_roles SELECT '00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-000000000010',id FROM roles WHERE name LIKE 'Overflow %';").execute(&db.pool).await.unwrap();
    assert!(matches!(
        db.store
            .introspect_key(probe(), super::tokens::ISSUER)
            .await,
        Err(darkhorse_domain::tokens::Error::Unavailable)
    ));
    assert_eq!(
        db.store.options([1; 32], None).await.err(),
        Some(Error::Unavailable)
    );
    sqlx::query("DELETE FROM principal_roles WHERE role_id IN (SELECT id FROM roles WHERE name LIKE 'Overflow %')").execute(&db.pool).await.unwrap();
    assert!(
        db.store
            .introspect_key(probe(), super::tokens::ISSUER)
            .await
            .unwrap()
            .is_some()
    );
    // Source-defined corruption fixtures bypass immutable guards only in this disposable database.
    sqlx::raw_sql("ALTER TABLE personal_key_grants DISABLE TRIGGER personal_key_grant_immutable; UPDATE personal_key_grants SET capability_ceiling=ARRAY['00000000-0000-0000-0000-000000000000']::uuid[]; ALTER TABLE personal_key_grants ENABLE TRIGGER personal_key_grant_immutable;").execute(&db.pool).await.unwrap();
    assert!(matches!(
        db.store
            .introspect_key(probe(), super::tokens::ISSUER)
            .await,
        Err(darkhorse_domain::tokens::Error::Unavailable)
    ));
    assert_eq!(
        Store::list(&db.store, [1; 32], None).await.err(),
        Some(Error::Unavailable)
    );
    sqlx::raw_sql("ALTER TABLE personal_key_grants DISABLE TRIGGER personal_key_grant_immutable; DELETE FROM personal_key_grants; ALTER TABLE personal_key_grants ENABLE TRIGGER personal_key_grant_immutable;").execute(&db.pool).await.unwrap();
    assert_eq!(
        Store::list(&db.store, [1; 32], None).await.err(),
        Some(Error::Unavailable)
    );
    sqlx::query("DROP TABLE security_state CASCADE")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(matches!(
        db.store
            .introspect_key(probe(), super::tokens::ISSUER)
            .await,
        Err(darkhorse_domain::tokens::Error::Unavailable)
    ));
    assert_eq!(
        Store::list(&db.store, [1; 32], None).await.err(),
        Some(Error::Unavailable)
    );
    db.store.close().await;
}
#[tokio::test]
async fn personal_keys_waiting_issuance_rechecks_committed_policy_and_session_revocation() {
    use darkhorse_application::personal_keys::Entropy;
    for revoke_session in [false, true] {
        let (db, _, _) = fixture().await;
        let input = request(&db, CapabilitySelection::All).await;
        Store::preflight(&db.store, [1; 32], &input).await.unwrap();
        let mut writer = db.pool.begin().await.unwrap();
        sqlx::query("SELECT singleton FROM security_state WHERE singleton FOR UPDATE")
            .execute(&mut *writer)
            .await
            .unwrap();
        let replica = db.store.clone();
        let pending = tokio::spawn(async move {
            Store::issue(
                &replica,
                [1; 32],
                &input,
                OsKeyEntropy.key().unwrap().verifier,
            )
            .await
        });
        for _ in 0..200 {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'SELECT singleton FROM security_state%FOR UPDATE')").fetch_one(&db.pool).await.unwrap();
            if waiting {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert!(!pending.is_finished());
        if revoke_session {
            sqlx::query("UPDATE browser_sessions SET revoked=true")
                .execute(&mut *writer)
                .await
                .unwrap();
        } else {
            sqlx::query("DELETE FROM role_capabilities")
                .execute(&mut *writer)
                .await
                .unwrap();
        }
        writer.commit().await.unwrap();
        assert_eq!(
            pending.await.unwrap().err(),
            Some(if revoke_session {
                Error::Unauthorized
            } else {
                Error::Conflict
            })
        );
        assert_eq!(total(&db).await, 0);
        db.store.close().await;
    }
}
