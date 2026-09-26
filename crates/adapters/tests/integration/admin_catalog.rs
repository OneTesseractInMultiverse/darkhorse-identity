use super::*;
use darkhorse_application::{
    admin_catalog::{CatalogStore, List, PolicyMapError, Target},
    authentication::AuthenticationStore,
};
use darkhorse_domain::{
    admin_catalog::{Change, PermissionDefinition, Query},
    identity::{ApplicationId, CapabilityId, ResourceId, RoleId, ScopeId},
    registration::{Label, RegistrationError as Error},
};
use std::num::NonZeroU128;
fn app() -> ApplicationId {
    ApplicationId::from_u128(16).unwrap()
}
fn cap() -> CapabilityId {
    CapabilityId::from_u128(128).unwrap()
}
fn role() -> RoleId {
    RoleId::from_u128(129).unwrap()
}
fn query() -> Query {
    Query {
        search: String::new(),
        active: None,
        after: None,
        limit: 25,
    }
}
async fn revision(db: &Database) -> u64 {
    db.store
        .list([1; 32], List::Applications, query())
        .await
        .unwrap()
        .policy_revision
}
async fn write(
    db: &Database,
    change: Change,
    new_id: Option<u128>,
) -> Result<darkhorse_application::admin_catalog::Written, Error> {
    db.store
        .execute(
            [1; 32],
            revision(db).await,
            change,
            new_id.map(|n| NonZeroU128::new(n).unwrap()),
        )
        .await
}
async fn definitions(db: &Database) {
    write(
        db,
        Change::CreateCapability {
            definition: PermissionDefinition::new("users.read", "Read the directory").unwrap(),
            application: Some(app()),
        },
        Some(128),
    )
    .await
    .unwrap();
    write(
        db,
        Change::CreateRole {
            name: Label::new("Reader").unwrap(),
            application: Some(app()),
        },
        Some(129),
    )
    .await
    .unwrap();
}
async fn audit_count(db: &Database) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM catalog_admin_audit")
        .fetch_one(&db.pool)
        .await
        .unwrap()
}
#[tokio::test]
async fn catalogs_are_bounded_and_searched_as_literals() {
    let db = oidc::fixture().await;
    sqlx::query("INSERT INTO applications(id,name,owner_id,active) SELECT lpad(to_hex(n),32,'0')::uuid,'Portal '||n,'00000000-0000-0000-0000-000000000001',true FROM generate_series(1000,2004) n").execute(&db.pool).await.unwrap();
    let mut q = query();
    q.limit = 100;
    let mut ids = std::collections::BTreeSet::new();
    loop {
        let page = db
            .store
            .list([1; 32], List::Applications, q.clone())
            .await
            .unwrap();
        assert!(page.items.len() <= 100);
        for item in page.items {
            assert!(ids.insert(item.id()));
        }
        match page.next {
            Some(id) => q.after = Some(id),
            None => break,
        }
    }
    assert_eq!(ids.len(), 1006);
    for search in ["%", "_", "\\", "' OR true --"] {
        let mut q = query();
        q.search = search.into();
        assert!(
            db.store
                .list([1; 32], List::Applications, q)
                .await
                .unwrap()
                .items
                .is_empty()
        );
    }
    for target in [
        List::Clients(app()),
        List::Resources(app()),
        List::Scopes(app()),
        List::Capabilities(None),
        List::Roles(None),
    ] {
        assert!(db.store.list([1; 32], target, query()).await.is_ok());
    }
    let mut q = query();
    q.active = Some(true);
    assert!(matches!(
        db.store.list([1; 32], List::Roles(None), q).await,
        Err(Error::Invalid)
    ));
    assert!(matches!(
        db.store
            .list(
                [1; 32],
                List::Clients(ApplicationId::from_u128(99999).unwrap()),
                query()
            )
            .await,
        Err(Error::NotFound)
    ));
    db.store.close().await;
}
#[tokio::test]
async fn ownership_never_grants_authority_and_failed_audit_rolls_back_every_effect() {
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
    sqlx::query("UPDATE applications SET owner_id=$1,revision=revision+1")
        .bind(Uuid::from_u128(2))
        .execute(&db.pool)
        .await
        .unwrap();
    let change = Change::CreateRole {
        name: Label::new("Reader").unwrap(),
        application: Some(app()),
    };
    let before = revision(&db).await;
    for (actor, error) in [([2; 32], Error::Forbidden), ([9; 32], Error::Unauthorized)] {
        assert!(matches!(db.store.list(actor,List::Applications,query()).await,Err(e) if e==error));
        assert_eq!(db.store.preflight(actor, before, &change).await, Err(error));
        assert!(
            matches!(db.store.execute(actor,before,change.clone(),NonZeroU128::new(129)).await,Err(e) if e==error)
        );
    }
    sqlx::raw_sql("CREATE FUNCTION reject_catalog_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected failure'; END; $$; CREATE TRIGGER reject_catalog_audit BEFORE INSERT ON catalog_admin_audit FOR EACH ROW EXECUTE FUNCTION reject_catalog_audit();").execute(&db.pool).await.unwrap();
    assert!(matches!(
        write(&db, change, Some(129)).await,
        Err(Error::Unavailable)
    ));
    assert_eq!(revision(&db).await, before);
    assert_eq!(audit_count(&db).await, 0);
    assert!(
        db.store
            .list([1; 32], List::Roles(None), query())
            .await
            .unwrap()
            .items
            .is_empty()
    );
    db.store.close().await;
}
#[tokio::test]
async fn shared_bindings_are_explicit_and_scope_bounds_cannot_expand_resource_authority() {
    let db = oidc::fixture().await;
    definitions(&db).await;
    let grant = Change::RoleCapability {
        role: role(),
        capability: cap(),
        granted: true,
    };
    write(&db, grant.clone(), None).await.unwrap();
    let before = revision(&db).await;
    let audit = audit_count(&db).await;
    write(&db, grant, None).await.unwrap();
    assert_eq!(revision(&db).await, before);
    assert_eq!(audit_count(&db).await, audit);
    sqlx::raw_sql("INSERT INTO applications(id,name,owner_id,active) VALUES('00000000-0000-0000-0000-000000000011','Other','00000000-0000-0000-0000-000000000001',true); INSERT INTO protected_resources(id,application_id,name,audience) VALUES('00000000-0000-0000-0000-000000000090','00000000-0000-0000-0000-000000000010','API','urn:darkhorse:resource:00000000-0000-0000-0000-000000000090'); INSERT INTO resource_scopes(id,application_id,resource_id,name) VALUES('00000000-0000-0000-0000-000000000091','00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000090','read');").execute(&db.pool).await.unwrap();
    let other = ApplicationId::from_u128(17).unwrap();
    assert!(
        db.store
            .list([1; 32], List::Roles(Some(other)), query())
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(matches!(
        write(
            &db,
            Change::RoleBinding {
                application: other,
                role: role(),
                bound: true
            },
            None
        )
        .await,
        Err(Error::Invalid)
    ));
    write(
        &db,
        Change::CapabilityBinding {
            application: other,
            capability: cap(),
            bound: true,
        },
        None,
    )
    .await
    .unwrap();
    write(
        &db,
        Change::RoleBinding {
            application: other,
            role: role(),
            bound: true,
        },
        None,
    )
    .await
    .unwrap();
    let resource = ResourceId::from_u128(144).unwrap();
    let scope = ScopeId::from_u128(145).unwrap();
    let bound = Change::ScopeCapability {
        application: app(),
        resource,
        scope,
        capability: cap(),
        included: true,
    };
    assert!(matches!(
        write(&db, bound.clone(), None).await,
        Err(Error::Invalid)
    ));
    write(
        &db,
        Change::ResourceCapability {
            application: app(),
            resource,
            capability: cap(),
            exposed: true,
        },
        None,
    )
    .await
    .unwrap();
    write(&db, bound, None).await.unwrap();
    assert_eq!(
        db.store
            .view([1; 32], Target::Scope(app(), resource, scope))
            .await
            .unwrap()
            .capabilities
            .len(),
        1
    );
    assert!(matches!(
        write(
            &db,
            Change::ResourceCapability {
                application: app(),
                resource,
                capability: cap(),
                exposed: false
            },
            None
        )
        .await,
        Err(Error::Invalid)
    ));
    assert!(matches!(
        write(
            &db,
            Change::CapabilityBinding {
                application: other,
                capability: cap(),
                bound: false
            },
            None
        )
        .await,
        Err(Error::Invalid)
    ));
    write(
        &db,
        Change::ScopeCapability {
            application: app(),
            resource,
            scope,
            capability: cap(),
            included: false,
        },
        None,
    )
    .await
    .unwrap();
    write(
        &db,
        Change::ResourceCapability {
            application: app(),
            resource,
            capability: cap(),
            exposed: false,
        },
        None,
    )
    .await
    .unwrap();
    write(&db, Change::RetireCapability(cap()), None)
        .await
        .unwrap();
    assert!(matches!(
        write(
            &db,
            Change::RoleCapability {
                role: role(),
                capability: cap(),
                granted: true
            },
            None
        )
        .await,
        Err(Error::Invalid)
    ));
    write(
        &db,
        Change::RoleCapability {
            role: role(),
            capability: cap(),
            granted: false,
        },
        None,
    )
    .await
    .unwrap();
    write(
        &db,
        Change::CapabilityBinding {
            application: other,
            capability: cap(),
            bound: false,
        },
        None,
    )
    .await
    .unwrap();
    for sql in [
        "DELETE FROM catalog_admin_audit",
        "UPDATE catalog_admin_audit SET event='role_created'",
        "UPDATE capabilities SET retired=false",
    ] {
        assert!(sqlx::query(sql).execute(&db.pool).await.is_err());
    }
    db.store.close().await;
}

#[tokio::test]
async fn policy_map_reads_one_authorized_application_snapshot_and_preserves_shared_binding_semantics()
 {
    let db = oidc::fixture().await;
    definitions(&db).await;
    write(
        &db,
        Change::RoleCapability {
            role: role(),
            capability: cap(),
            granted: true,
        },
        None,
    )
    .await
    .unwrap();
    let other = ApplicationId::from_u128(17).unwrap();
    sqlx::query("INSERT INTO applications(id,name,owner_id,active) VALUES($1,'Other','00000000-0000-0000-0000-000000000001',true)")
        .bind(Uuid::from_u128(other.as_u128()))
        .execute(&db.pool)
        .await
        .unwrap();

    assert!(matches!(
        db.store.policy_map([9; 32], app()).await,
        Err(PolicyMapError::Unauthorized)
    ));
    let initially_unbound = db.store.policy_map([1; 32], other).await.unwrap();
    assert!(initially_unbound.roles.is_empty());
    assert!(initially_unbound.capabilities.is_empty());

    write(
        &db,
        Change::CapabilityBinding {
            application: other,
            capability: cap(),
            bound: true,
        },
        None,
    )
    .await
    .unwrap();
    write(
        &db,
        Change::RoleBinding {
            application: other,
            role: role(),
            bound: true,
        },
        None,
    )
    .await
    .unwrap();

    let first_resource = ResourceId::from_u128(144).unwrap();
    let first_scope = ScopeId::from_u128(145).unwrap();
    let second_resource = ResourceId::from_u128(146).unwrap();
    let second_scope = ScopeId::from_u128(147).unwrap();
    sqlx::query("INSERT INTO protected_resources(id,application_id,name,audience) VALUES($1,$2,'First API',$3),($4,$5,'Second API',$6)")
        .bind(Uuid::from_u128(first_resource.as_u128()))
        .bind(Uuid::from_u128(app().as_u128()))
        .bind(format!("urn:darkhorse:resource:{}", Uuid::from_u128(first_resource.as_u128())))
        .bind(Uuid::from_u128(second_resource.as_u128()))
        .bind(Uuid::from_u128(other.as_u128()))
        .bind(format!("urn:darkhorse:resource:{}", Uuid::from_u128(second_resource.as_u128())))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO resource_scopes(id,application_id,resource_id,name) VALUES($1,$2,$3,'read'),($4,$5,$6,'write')")
        .bind(Uuid::from_u128(first_scope.as_u128()))
        .bind(Uuid::from_u128(app().as_u128()))
        .bind(Uuid::from_u128(first_resource.as_u128()))
        .bind(Uuid::from_u128(second_scope.as_u128()))
        .bind(Uuid::from_u128(other.as_u128()))
        .bind(Uuid::from_u128(second_resource.as_u128()))
        .execute(&db.pool)
        .await
        .unwrap();
    for (application, resource, scope) in [
        (app(), first_resource, first_scope),
        (other, second_resource, second_scope),
    ] {
        write(
            &db,
            Change::ResourceCapability {
                application,
                resource,
                capability: cap(),
                exposed: true,
            },
            None,
        )
        .await
        .unwrap();
        write(
            &db,
            Change::ScopeCapability {
                application,
                resource,
                scope,
                capability: cap(),
                included: true,
            },
            None,
        )
        .await
        .unwrap();
    }

    let first = db.store.policy_map([1; 32], app()).await.unwrap();
    let second = db.store.policy_map([1; 32], other).await.unwrap();
    assert_eq!(first.application.id, app());
    assert_eq!(first.policy_revision, revision(&db).await);
    assert_eq!(
        first.roles.iter().map(|value| value.id).collect::<Vec<_>>(),
        [role()]
    );
    assert_eq!(
        first
            .capabilities
            .iter()
            .map(|value| value.id)
            .collect::<Vec<_>>(),
        [cap()]
    );
    assert_eq!(
        first
            .resources
            .iter()
            .map(|value| value.id)
            .collect::<Vec<_>>(),
        [first_resource]
    );
    assert_eq!(
        first
            .scopes
            .iter()
            .map(|value| value.id)
            .collect::<Vec<_>>(),
        [first_scope]
    );
    assert_eq!(first.edges.len(), 7);
    assert_eq!(second.application.id, other);
    assert_eq!(
        second
            .resources
            .iter()
            .map(|value| value.id)
            .collect::<Vec<_>>(),
        [second_resource]
    );
    assert_eq!(
        second
            .scopes
            .iter()
            .map(|value| value.id)
            .collect::<Vec<_>>(),
        [second_scope]
    );
    assert_eq!(second.edges.len(), 7);

    assert!(matches!(
        db.store
            .policy_map([1; 32], ApplicationId::from_u128(999).unwrap())
            .await,
        Err(PolicyMapError::NotFound)
    ));
    db.store.logout([1; 32]).await.unwrap();
    assert!(matches!(
        db.store.policy_map([1; 32], app()).await,
        Err(PolicyMapError::Unauthorized)
    ));
    db.store.close().await;
}

#[tokio::test]
async fn policy_map_database_read_stops_at_its_bounded_node_limit() {
    let db = oidc::fixture().await;
    sqlx::raw_sql("INSERT INTO protected_resources(id,application_id,name,audience) SELECT lpad(to_hex(n),32,'0')::uuid,'00000000-0000-0000-0000-000000000010','Resource '||n,'urn:darkhorse:resource:'||(lpad(to_hex(n),32,'0')::uuid)::text FROM generate_series(1000,3047) n")
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        db.store.policy_map([1; 32], app()).await,
        Err(PolicyMapError::TooLarge)
    );
    db.store.close().await;
}

#[tokio::test]
async fn policy_map_waits_for_the_security_fence_and_returns_matching_committed_revision() {
    let db = oidc::fixture().await;
    let mut writer = db.pool.begin().await.unwrap();
    sqlx::query("SELECT singleton FROM security_state WHERE singleton FOR UPDATE")
        .execute(&mut *writer)
        .await
        .unwrap();

    let store = db.store.clone();
    let reader = tokio::spawn(async move { store.policy_map([1; 32], app()).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE '%security_state%FOR SHARE%')")
                .fetch_one(&db.pool)
                .await
                .unwrap();
            if waiting {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    sqlx::query("INSERT INTO roles(id,name) VALUES($1,'Committed role')")
        .bind(Uuid::from_u128(230))
        .execute(&mut *writer)
        .await
        .unwrap();
    sqlx::query("INSERT INTO role_applications(application_id,role_id) VALUES($1,$2)")
        .bind(Uuid::from_u128(app().as_u128()))
        .bind(Uuid::from_u128(230))
        .execute(&mut *writer)
        .await
        .unwrap();
    writer.commit().await.unwrap();

    let graph = reader.await.unwrap().unwrap();
    let current_revision = revision(&db).await;
    assert_eq!(graph.policy_revision, current_revision);
    assert!(
        graph
            .roles
            .iter()
            .any(|value| value.id == RoleId::from_u128(230).unwrap())
    );
    db.store.close().await;
}
#[tokio::test]
async fn concurrent_catalog_edits_have_one_winner_and_stale_or_expired_authority_never_writes() {
    let db = oidc::fixture().await;
    definitions(&db).await;
    let before = revision(&db).await;
    let change = Change::RoleCapability {
        role: role(),
        capability: cap(),
        granted: true,
    };
    let (a, b) = tokio::join!(
        db.store.execute([1; 32], before, change.clone(), None),
        db.store
            .execute([1; 32], before, Change::RetireCapability(cap()), None)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(matches!(a, Err(Error::Conflict)) || matches!(b, Err(Error::Conflict)));
    assert_eq!(
        db.store.preflight([1; 32], before, &change).await,
        Err(Error::Conflict)
    );
    let audit = audit_count(&db).await;
    sqlx::raw_sql("ALTER TABLE browser_sessions DISABLE TRIGGER browser_session_transition; UPDATE browser_sessions SET created_ms=created_ms-300000,expires_ms=expires_ms-300000; ALTER TABLE browser_sessions ENABLE TRIGGER browser_session_transition;").execute(&db.pool).await.unwrap();
    assert!(matches!(
        write(&db, change, None).await,
        Err(Error::RecentAuthenticationRequired)
    ));
    assert_eq!(audit_count(&db).await, audit);
    db.store.close().await;
}
#[tokio::test]
async fn catalog_commands_immediately_reduce_introspection_and_never_expand_issued_ceilings() {
    use super::tokens::{ISSUER, input, setup};
    use darkhorse_adapters::{
        resource_servers::OsResourceEntropy,
        tokens::material::{self, Purpose},
    };
    use darkhorse_application::{
        resource_servers as rs,
        resource_servers::{Registry, ResourceTokenStore},
        tokens::{CodeStore, TokenStore},
    };
    let (db, signer) = setup().await;
    super::resource_tokens::policy(&db).await;
    let resource = ResourceId::from_u128(0x30).unwrap();
    let role = RoleId::from_u128(0x80).unwrap();
    let capability = CapabilityId::from_u128(0x71).unwrap();
    let registry = rs::Service {
        store: db.store.clone(),
        entropy: OsResourceEntropy,
    };
    let registered = registry
        .write(
            [1; 32],
            rs::Command {
                target: rs::Target {
                    application: app(),
                    resource,
                },
                change: rs::Change::Register,
            },
        )
        .await
        .unwrap();
    let verifier =
        darkhorse_adapters::resource_servers::secret_digest(&registered.secret.unwrap()).unwrap();
    super::resource_tokens::approve(&db, [3; 32]).await;
    write(
        &db,
        Change::RoleCapability {
            role,
            capability,
            granted: false,
        },
        None,
    )
    .await
    .unwrap();
    let code = db
        .store
        .issue(
            [3; 32],
            Some([1; 32]),
            material::generate(Purpose::Code).unwrap(),
        )
        .await
        .unwrap();
    write(
        &db,
        Change::RoleCapability {
            role,
            capability,
            granted: true,
        },
        None,
    )
    .await
    .unwrap();
    let token = db
        .store
        .redeem(input(&code), material::pair().unwrap(), ISSUER, &signer)
        .await
        .unwrap();
    let digest = material::digest(&token.access, Purpose::Access).unwrap();
    let probe = || rs::Probe {
        resource,
        secret: verifier,
        token: Some(digest),
    };
    let checked = db
        .store
        .introspect_resource(probe(), ISSUER)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        checked.capabilities,
        std::collections::BTreeSet::from([CapabilityId::from_u128(0x70).unwrap()])
    );
    write(
        &db,
        Change::RetireCapability(CapabilityId::from_u128(0x70).unwrap()),
        None,
    )
    .await
    .unwrap();
    assert!(
        db.store
            .introspect_resource(probe(), ISSUER)
            .await
            .unwrap()
            .is_none()
    );
    db.store.close().await;
}
#[tokio::test]
async fn catalog_upgrade_preserves_existing_authority_without_fabricating_actor_history() {
    let db = Database::at_version(17).await;
    db.store
        .bootstrap(administrator(1, "one@example.com"))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO capabilities(id,permission_key,meaning) VALUES($1,'read','Read records')",
    )
    .bind(Uuid::from_u128(128))
    .execute(&db.pool)
    .await
    .unwrap();
    let before: i64 = sqlx::query_scalar("SELECT policy_revision FROM security_state")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    db.store.migrate().await.unwrap();
    db.store.migrate().await.unwrap();
    let after: i64 = sqlx::query_scalar("SELECT policy_revision FROM security_state")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(audit_count(&db).await, 0);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT meaning FROM capabilities")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        "Read records"
    );
    db.store.close().await;
}
