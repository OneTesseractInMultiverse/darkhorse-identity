use super::operator_directory::{Allow, Held, Password};
use super::*;
use darkhorse_application::{
    admin_catalog::{CatalogStore, List, Page},
    operator_accounts::{self, Store},
};
use darkhorse_domain::{
    admin_catalog::Query,
    identity::{ApplicationId, OperationId},
    operator_accounts::Error,
    operator_catalog::{Request, Target},
};
fn query() -> Query {
    Query {
        search: String::new(),
        active: None,
        after: None,
        limit: 25,
    }
}
async fn list(
    store: &impl Store<Request = Request, Outcome = Page>,
    email: &str,
    target: Target,
    query: Query,
) -> Result<Page, Error> {
    operator_accounts::run(
        store,
        &Allow,
        &Password(true),
        OperationId::from_u128(Uuid::new_v4().as_u128()).unwrap(),
        Request::new(target, query).unwrap(),
        email,
        "source-only passphrase",
    )
    .await
}
#[tokio::test]
async fn catalog_reads_share_console_pagination_filtering_and_application_isolation() {
    let db = oidc::fixture().await;
    sqlx::raw_sql("INSERT INTO applications(id,name,owner_id,active) SELECT lpad(to_hex(n),32,'0')::uuid,'Portal '||n,'00000000-0000-0000-0000-000000000001',true FROM generate_series(1000,1053) n; INSERT INTO oauth_clients(id,application_id,name,active) SELECT lpad(to_hex(n),32,'0')::uuid,'00000000-0000-0000-0000-000000000010','Console '||n,true FROM generate_series(1000,1053) n; INSERT INTO oauth_clients(id,application_id,name,active) VALUES('00000000-0000-0000-0000-000000000099','00000000-0000-0000-0000-0000000003e8','Foreign',false);") .execute(&db.pool).await.unwrap();
    for (target, console) in [
        (Target::Applications, List::Applications),
        (
            Target::Clients(ApplicationId::from_u128(16).unwrap()),
            List::Clients(ApplicationId::from_u128(16).unwrap()),
        ),
    ] {
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
            let expected = db.store.list([1; 32], console, q.clone()).await.unwrap();
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
            if let Some(next) = page.next {
                q.after = Some(next);
            } else {
                break;
            }
        }
        assert_eq!(ids.len(), 55);
        for search in ["%", "_", "\\", "' OR true --", "audit-secret-marker"] {
            assert!(
                list(
                    &db.store.operator_catalog(),
                    "one@example.com",
                    target,
                    Query {
                        search: search.into(),
                        ..query()
                    }
                )
                .await
                .unwrap()
                .items
                .is_empty()
            );
        }
        assert!(
            list(
                &db.store.operator_catalog(),
                "one@example.com",
                target,
                Query {
                    active: Some(false),
                    ..query()
                }
            )
            .await
            .unwrap()
            .items
            .is_empty()
        );
    }
    let found = list(
        &db.store.operator_catalog(),
        "one@example.com",
        Target::Applications,
        Query {
            search: "portal 1000".into(),
            ..query()
        },
    )
    .await
    .unwrap();
    assert_eq!(found.items.len(), 1);
    let foreign = list(
        &db.store.operator_catalog(),
        "one@example.com",
        Target::Clients(ApplicationId::from_u128(1000).unwrap()),
        Query {
            active: Some(false),
            ..query()
        },
    )
    .await
    .unwrap();
    assert_eq!(foreign.items.len(), 1);
    assert_eq!(foreign.items[0].id().get(), 153);
    sqlx::query("UPDATE applications SET active=false,revision=revision+1 WHERE id=$1")
        .bind(Uuid::from_u128(16))
        .execute(&db.pool)
        .await
        .unwrap();
    let inactive = list(
        &db.store.operator_catalog(),
        "one@example.com",
        Target::Applications,
        Query {
            active: Some(false),
            ..query()
        },
    )
    .await
    .unwrap();
    assert_eq!(inactive.items.len(), 1);
    assert_eq!(inactive.items[0].id().get(), 16);
    let clients = list(
        &db.store.operator_catalog(),
        "one@example.com",
        Target::Clients(ApplicationId::from_u128(16).unwrap()),
        Query {
            active: Some(true),
            ..query()
        },
    )
    .await
    .unwrap();
    assert_eq!(clients.items.len(), 25);
    let audits: Vec<String> =
        sqlx::query_scalar("SELECT row_to_json(a)::text FROM operator_catalog_audit a")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert!(audits.iter().all(|v| !v.contains("audit-secret-marker")
        && !v.contains("one@example.com")
        && !v.contains("Portal")));
}
#[tokio::test]
async fn ownership_is_not_administration_and_missing_targets_are_audited_only_after_authentication()
{
    let db = oidc::fixture().await;
    insert_principal(&db, 2, false).await;
    insert_password(&db, 2).await;
    sqlx::query("UPDATE applications SET revision=revision+1,owner_id=$1")
        .bind(Uuid::from_u128(2))
        .execute(&db.pool)
        .await
        .unwrap();
    for email in ["person2@example.com", "missing@example.com"] {
        assert!(matches!(
            list(
                &db.store.operator_catalog(),
                email,
                Target::Clients(ApplicationId::from_u128(16).unwrap()),
                query()
            )
            .await,
            Err(Error::Denied)
        ));
    }
    assert!(matches!(
        list(
            &db.store.operator_catalog(),
            "one@example.com",
            Target::Clients(ApplicationId::from_u128(999).unwrap()),
            query()
        )
        .await,
        Err(Error::NotFound)
    ));
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM operator_catalog_audit WHERE result='not_found' AND application_id='00000000-0000-0000-0000-0000000003e7' AND actor_id IS NOT NULL AND returned_count IS NULL").fetch_one(&db.pool).await.unwrap(),1);
}
#[tokio::test]
async fn catalog_queries_waiting_on_security_writes_recheck_demoted_or_expired_actors() {
    for age in [0, 60_000] {
        let db = oidc::fixture().await;
        insert_principal(&db, 3, true).await;
        let held = Held {
            inner: db.store.operator_catalog(),
            ready: tokio::sync::Notify::new(),
            resume: tokio::sync::Notify::new(),
            age,
        };
        let mut tx = db.pool.begin().await.unwrap();
        sqlx::query("SELECT singleton FROM security_state FOR UPDATE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let read = list(&held, "one@example.com", Target::Applications, query());
        let reduce = async {
            held.ready.notified().await;
            held.resume.notify_one();
            tokio::time::timeout(std::time::Duration::from_secs(5),async {
                loop {
                    let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE '%security_state%')").fetch_one(&db.pool).await.unwrap();
                    if waiting {break;} tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            }).await.unwrap();
            if age == 0 {
                sqlx::query("DELETE FROM platform_administrators WHERE principal_id=$1")
                    .bind(Uuid::from_u128(1))
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            tx.commit().await.unwrap();
        };
        let (result, ()) = tokio::join!(read, reduce);
        assert!(matches!(result, Err(Error::Denied)));
    }
}
#[tokio::test]
async fn catalog_audit_failure_and_suppression_release_no_results() {
    for body in ["RAISE EXCEPTION 'fixture';", "RETURN NULL;"] {
        let db = oidc::fixture().await;
        let sql = format!(
            "CREATE FUNCTION deny_catalog_read() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN {body} END $$; CREATE TRIGGER deny_catalog_read BEFORE INSERT ON operator_catalog_audit FOR EACH ROW EXECUTE FUNCTION deny_catalog_read();"
        );
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
            .execute(&db.pool)
            .await
            .unwrap();
        for email in ["one@example.com", "missing@example.com"] {
            assert!(matches!(
                list(
                    &db.store.operator_catalog(),
                    email,
                    Target::Applications,
                    query()
                )
                .await,
                Err(Error::Unavailable)
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
}
#[tokio::test]
async fn failed_or_lost_catalog_commit_never_releases_a_page_or_retries() {
    let db = oidc::fixture().await;
    sqlx::raw_sql("CREATE FUNCTION reject_catalog_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture'; END $$; CREATE CONSTRAINT TRIGGER reject_catalog_commit AFTER INSERT ON operator_catalog_audit DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_catalog_commit();").execute(&db.pool).await.unwrap();
    assert!(matches!(
        list(
            &db.store.operator_catalog(),
            "one@example.com",
            Target::Applications,
            query()
        )
        .await,
        Err(Error::Uncertain)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM operator_catalog_audit")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query("DROP TRIGGER reject_catalog_commit ON operator_catalog_audit")
        .execute(&db.pool)
        .await
        .unwrap();
    let (store, proxy) = super::limiter_activation::lost_nth_commit(&db.pool, 1).await;
    assert!(matches!(
        list(
            &store.operator_catalog(),
            "one@example.com",
            Target::Applications,
            query()
        )
        .await,
        Err(Error::Uncertain)
    ));
    proxy.await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM operator_catalog_audit WHERE result='read'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        1
    );
    store.close().await;
}
