use super::operator_catalog::{list, query};
use super::*;
use darkhorse_application::admin_catalog::{CatalogStore, List};
use darkhorse_domain::{
    identity::ApplicationId,
    operator_accounts::Error,
    operator_catalog::{Definitions, Target},
};
fn app() -> ApplicationId {
    ApplicationId::from_u128(16).unwrap()
}
fn targets() -> [(Target, List); 6] {
    [
        (Target::Resources(app()), List::Resources(app())),
        (Target::Scopes(app()), List::Scopes(app())),
        (
            Target::Roles(Definitions::Application(app())),
            List::Roles(Some(app())),
        ),
        (
            Target::Capabilities(Definitions::Application(app())),
            List::Capabilities(Some(app())),
        ),
        (Target::Roles(Definitions::All), List::Roles(None)),
        (
            Target::Capabilities(Definitions::All),
            List::Capabilities(None),
        ),
    ]
}
async fn fixture() -> Database {
    let db = oidc::fixture().await;
    sqlx::raw_sql("INSERT INTO protected_resources(id,application_id,name,audience) SELECT lpad(to_hex(n),32,'0')::uuid,'00000000-0000-0000-0000-000000000010','Resource '||n,'urn:darkhorse:resource:'||lpad(to_hex(n),32,'0')::uuid::text FROM generate_series(1000,1053) n;
INSERT INTO resource_scopes(id,application_id,resource_id,name) SELECT lpad(to_hex(n),32,'0')::uuid,'00000000-0000-0000-0000-000000000010',lpad(to_hex(n),32,'0')::uuid,'scope:'||n FROM generate_series(1000,1053) n;
INSERT INTO capabilities(id,permission_key,meaning) SELECT lpad(to_hex(n),32,'0')::uuid,'capability:'||n,'private-description-marker' FROM generate_series(1000,1053) n;
INSERT INTO roles(id,name) SELECT lpad(to_hex(n),32,'0')::uuid,'Role '||n FROM generate_series(1000,1053) n;
INSERT INTO capability_applications SELECT '00000000-0000-0000-0000-000000000010',id FROM capabilities;
INSERT INTO role_applications SELECT '00000000-0000-0000-0000-000000000010',id FROM roles;
UPDATE capabilities SET retired=true WHERE id='00000000-0000-0000-0000-0000000003e8';
INSERT INTO applications(id,name,owner_id,active) VALUES('00000000-0000-0000-0000-000000000011','Other','00000000-0000-0000-0000-000000000001',true);
INSERT INTO roles(id,name) VALUES('00000000-0000-0000-0000-000000000100','Unbound'),('00000000-0000-0000-0000-000000000101','Foreign');
INSERT INTO capabilities(id,permission_key,meaning) VALUES('00000000-0000-0000-0000-000000000100','unbound','Definition only'),('00000000-0000-0000-0000-000000000101','foreign','Other app only');
INSERT INTO role_applications VALUES('00000000-0000-0000-0000-000000000011','00000000-0000-0000-0000-000000000101');
INSERT INTO capability_applications VALUES('00000000-0000-0000-0000-000000000011','00000000-0000-0000-0000-000000000101');").execute(&db.pool).await.unwrap();
    db
}
#[tokio::test]
async fn access_pages_match_http_queries_without_implicit_bindings_or_wildcards() {
    let db = fixture().await;
    for (target, http) in targets() {
        let mut q = query();
        let mut ids = std::collections::BTreeSet::new();
        loop {
            let page = list(
                &db.store.operator_catalog(),
                "one@example.com",
                target,
                q.clone(),
            )
            .await
            .unwrap();
            let expected = db.store.list([1; 32], http, q.clone()).await.unwrap();
            assert_eq!(
                page.items.iter().map(|i| i.id()).collect::<Vec<_>>(),
                expected.items.iter().map(|i| i.id()).collect::<Vec<_>>()
            );
            assert_eq!(page.next, expected.next);
            assert_eq!(page.policy_revision, expected.policy_revision);
            assert!(page.items.len() <= 25);
            for item in page.items {
                assert!(ids.insert(item.id()));
            }
            match page.next {
                Some(id) => q.after = Some(id),
                None => break,
            }
        }
        let all = matches!(
            target,
            Target::Roles(Definitions::All) | Target::Capabilities(Definitions::All)
        );
        assert_eq!(ids.len(), if all { 56 } else { 54 });
        assert_eq!(ids.iter().any(|id| id.get() == 256), all);
        assert_eq!(ids.iter().any(|id| id.get() == 257), all);
        for search in ["%", "_", "\\", "' OR true --", "audit-private-marker"] {
            let q = darkhorse_domain::admin_catalog::Query {
                search: search.into(),
                ..query()
            };
            assert!(
                list(&db.store.operator_catalog(), "one@example.com", target, q)
                    .await
                    .unwrap()
                    .items
                    .is_empty()
            );
        }
    }
    for active in [true, false] {
        let q = darkhorse_domain::admin_catalog::Query {
            active: Some(active),
            ..query()
        };
        let page = list(
            &db.store.operator_catalog(),
            "one@example.com",
            Target::Capabilities(Definitions::Application(app())),
            q,
        )
        .await
        .unwrap();
        assert_eq!(page.items.len(), if active { 25 } else { 1 });
    }
    // Explicit binding removal changes the next primary read; all-definitions still contains the definition.
    sqlx::query(
        "DELETE FROM role_applications WHERE role_id='00000000-0000-0000-0000-0000000003e8'",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    let q = darkhorse_domain::admin_catalog::Query {
        search: "Role 1000".into(),
        ..query()
    };
    assert!(
        list(
            &db.store.operator_catalog(),
            "one@example.com",
            Target::Roles(Definitions::Application(app())),
            q.clone()
        )
        .await
        .unwrap()
        .items
        .is_empty()
    );
    assert_eq!(
        list(
            &db.store.operator_catalog(),
            "one@example.com",
            Target::Roles(Definitions::All),
            q
        )
        .await
        .unwrap()
        .items
        .len(),
        1
    );
    let audits: Vec<String> =
        sqlx::query_scalar("SELECT row_to_json(a)::text FROM operator_catalog_audit a")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert!(audits.iter().all(|v| {
        ![
            "one@example.com",
            "private-description-marker",
            "audit-private-marker",
            "Role 1000",
        ]
        .iter()
        .any(|m| v.contains(m))
    }));
}
#[tokio::test]
async fn access_missing_applications_and_ownership_follow_current_admin_authority() {
    let db = fixture().await;
    let missing = ApplicationId::from_u128(999).unwrap();
    for target in [
        Target::Resources(missing),
        Target::Scopes(missing),
        Target::Roles(Definitions::Application(missing)),
        Target::Capabilities(Definitions::Application(missing)),
    ] {
        assert!(matches!(
            list(
                &db.store.operator_catalog(),
                "one@example.com",
                target,
                query()
            )
            .await,
            Err(Error::NotFound)
        ));
    }
    insert_principal(&db, 3, true).await;
    sqlx::query("DELETE FROM platform_administrators WHERE principal_id=$1")
        .bind(Uuid::from_u128(1))
        .execute(&db.pool)
        .await
        .unwrap();
    for (target, _) in targets() {
        assert!(matches!(
            list(
                &db.store.operator_catalog(),
                "one@example.com",
                target,
                query()
            )
            .await,
            Err(Error::Denied)
        ));
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM operator_catalog_audit WHERE result='not_found'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        4
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM operator_catalog_audit WHERE result='denied'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        6
    );
}
#[tokio::test]
async fn audit_time_authority_loss_releases_no_catalog_page() {
    let db = fixture().await;
    insert_principal(&db, 3, true).await;
    sqlx::raw_sql("CREATE FUNCTION demote_catalog_actor() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN DELETE FROM platform_administrators WHERE principal_id=NEW.actor_id; RETURN NEW; END $$; CREATE TRIGGER demote_catalog_actor BEFORE INSERT ON operator_catalog_audit FOR EACH ROW EXECUTE FUNCTION demote_catalog_actor();").execute(&db.pool).await.unwrap();
    for target in targets()
        .map(|(t, _)| t)
        .into_iter()
        .chain([Target::Applications, Target::Clients(app())])
    {
        assert!(matches!(
            list(
                &db.store.operator_catalog(),
                "one@example.com",
                target,
                query()
            )
            .await,
            Err(Error::Denied)
        ));
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM operator_catalog_audit")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
}
#[tokio::test]
async fn access_audit_failure_and_lost_commit_release_no_page() {
    let db = fixture().await;
    for body in ["RETURN NULL;", "RAISE EXCEPTION 'fixture';"] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION reject_access_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN {body} END $$; CREATE TRIGGER reject_access_audit BEFORE INSERT ON operator_catalog_audit FOR EACH ROW EXECUTE FUNCTION reject_access_audit();"))).execute(&db.pool).await.unwrap();
        for (target, _) in targets() {
            assert!(matches!(
                list(
                    &db.store.operator_catalog(),
                    "one@example.com",
                    target,
                    query()
                )
                .await,
                Err(Error::Unavailable)
            ));
        }
        sqlx::raw_sql("DROP TRIGGER reject_access_audit ON operator_catalog_audit; DROP FUNCTION reject_access_audit();").execute(&db.pool).await.unwrap();
    }
    let (store, proxy) = super::limiter_activation::lost_nth_commit(&db.pool, 1).await;
    assert!(matches!(
        list(
            &store.operator_catalog(),
            "one@example.com",
            Target::Roles(Definitions::All),
            query()
        )
        .await,
        Err(Error::Uncertain)
    ));
    proxy.await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM operator_catalog_audit")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    store.close().await;
}
#[tokio::test]
async fn access_audit_upgrade_preserves_history_and_rejects_incoherent_selectors() {
    let db = Database::at_version(30).await;
    sqlx::query("INSERT INTO operator_catalog_audit(operation_id,command,query_limit,searched,result,occurred_ms) VALUES($1,'application.list',25,false,'denied',0)").bind(Uuid::from_u128(1)).execute(&db.pool).await.unwrap();
    let before: String =
        sqlx::query_scalar("SELECT row_to_json(a)::text FROM operator_catalog_audit a")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    db.store.migrate().await.unwrap();
    let after: String =
        sqlx::query_scalar("SELECT row_to_json(a)::text FROM operator_catalog_audit a")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    for (command, application, active, valid) in [
        ("resource.list", Some(16), None, true),
        ("scope.list", Some(16), None, true),
        ("role.list", None, None, true),
        ("capability.list", None, Some(false), true),
        ("application.list", Some(16), None, false),
        ("client.list", None, None, false),
        ("resource.list", None, None, false),
        ("scope.list", None, None, false),
        ("role.list", None, Some(true), false),
        ("resource.list", Some(16), Some(false), false),
        ("scope.list", Some(16), Some(true), false),
        ("unsupported.list", None, None, false),
    ] {
        let result=sqlx::query("INSERT INTO operator_catalog_audit(operation_id,command,application_id,query_limit,active_filter,searched,result,occurred_ms) VALUES($1,$2,$3,25,$4,false,'denied',0)").bind(Uuid::new_v4()).bind(command).bind(application.map(Uuid::from_u128)).bind(active).execute(&db.pool).await;
        assert_eq!(
            result.is_ok(),
            valid,
            "{command} {application:?} {active:?}"
        );
    }
    assert!(
        sqlx::query("UPDATE operator_catalog_audit SET query_limit=1")
            .execute(&db.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM operator_catalog_audit")
            .execute(&db.pool)
            .await
            .is_err()
    );
}
