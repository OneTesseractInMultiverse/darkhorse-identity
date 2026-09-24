use super::*;
use darkhorse_application::{
    admin_directory::{AdminDirectory, Page},
    authentication::{AuthError, LoginAdmission, PasswordVerification},
    operator_accounts::{self, CandidateAt, Store, Verified},
};
use darkhorse_domain::{
    admin_directory::Query, identity::OperationId, operator_accounts::Error,
    operator_directory::Request,
};
struct Allow;
impl LoginAdmission for Allow {
    async fn admit(&self, _: &str) -> Result<(), AuthError> {
        Ok(())
    }
}
struct Password(bool);
impl PasswordVerification for Password {
    async fn verify(&self, _: &str, _: Option<&str>) -> Result<bool, AuthError> {
        Ok(self.0)
    }
}
fn query() -> Query {
    Query {
        search: String::new(),
        status: None,
        after: None,
        limit: 25,
    }
}
async fn list(
    store: &impl Store<Request = Request, Outcome = Page>,
    email: &str,
    query: Query,
) -> Result<Page, Error> {
    operator_accounts::run(
        store,
        &Allow,
        &Password(true),
        OperationId::from_u128(Uuid::new_v4().as_u128()).unwrap(),
        Request::new(query).unwrap(),
        email,
        "source-only passphrase",
    )
    .await
}
#[tokio::test]
async fn authenticated_directory_pages_match_the_console_query_without_duplicates_or_wildcards() {
    let db = oidc::fixture().await;
    sqlx::query("INSERT INTO principals(id,email,first_name,last_name) SELECT lpad(to_hex(n),32,'0')::uuid,'listed'||n||'@example.com','Listed','Person' FROM generate_series(1000,1053) n").execute(&db.pool).await.unwrap();
    let reader = db.store.operator_directory();
    let mut q = query();
    let mut ids = std::collections::BTreeSet::new();
    loop {
        let page = list(&reader, "one@example.com", q.clone()).await.unwrap();
        let console = db.store.users([1; 32], q.clone()).await.unwrap();
        assert_eq!(
            page.items.iter().map(|u| u.id).collect::<Vec<_>>(),
            console.items.iter().map(|u| u.id).collect::<Vec<_>>()
        );
        assert_eq!(page.next, console.next);
        assert!(page.items.len() <= 25);
        for user in page.items {
            assert!(ids.insert(user.id));
        }
        match page.next {
            Some(next) => q.after = Some(next),
            None => break,
        }
    }
    assert_eq!(ids.len(), 55);
    for search in ["%", "_", "\\", "' OR true --", "secret-search-marker"] {
        assert!(
            list(
                &reader,
                "one@example.com",
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
    let empty = list(
        &reader,
        "one@example.com",
        Query {
            status: Some(AccountStatus::Inactive),
            ..query()
        },
    )
    .await
    .unwrap();
    assert!(empty.items.is_empty());
    let found = list(
        &reader,
        "one@example.com",
        Query {
            search: "LOVE".into(),
            ..query()
        },
    )
    .await
    .unwrap();
    assert_eq!(found.items[0].id, id(1));
    let records: Vec<String> =
        sqlx::query_scalar("SELECT row_to_json(a)::text FROM operator_directory_audit a")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert!(
        records
            .iter()
            .all(|r| !r.contains("secret-search-marker") && !r.contains("one@example.com"))
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM operator_directory_audit WHERE actor_id IS NULL OR result<>'read' OR returned_count>query_limit").fetch_one(&db.pool).await.unwrap(), 0);
}

#[tokio::test]
async fn directory_denials_and_audit_failures_never_release_profile_data() {
    let db = oidc::fixture().await;
    insert_principal(&db, 2, false).await;
    insert_password(&db, 2).await;
    let reader = db.store.operator_directory();
    for email in ["person2@example.com", "missing@example.com"] {
        assert!(matches!(
            list(&reader, email, query()).await,
            Err(Error::Denied)
        ));
    }
    assert!(matches!(
        operator_accounts::run(
            &reader,
            &Allow,
            &Password(false),
            OperationId::from_u128(1).unwrap(),
            Request::new(query()).unwrap(),
            "one@example.com",
            "wrong password"
        )
        .await,
        Err(Error::Denied)
    ));
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM operator_directory_audit WHERE result='denied' AND returned_count IS NULL").fetch_one(&db.pool).await.unwrap(), 3);
    sqlx::raw_sql("CREATE FUNCTION reject_directory_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture'; END $$; CREATE TRIGGER reject_directory_audit BEFORE INSERT ON operator_directory_audit FOR EACH ROW EXECUTE FUNCTION reject_directory_audit();").execute(&db.pool).await.unwrap();
    assert!(matches!(
        list(&reader, "one@example.com", query()).await,
        Err(Error::Unavailable)
    ));
    assert!(matches!(
        list(&reader, "missing@example.com", query()).await,
        Err(Error::Unavailable)
    ));
}

struct Held<S> {
    inner: S,
    ready: tokio::sync::Notify,
    resume: tokio::sync::Notify,
    age: u64,
}
impl<S: Store> Store for Held<S> {
    type Request = S::Request;
    type Outcome = S::Outcome;
    async fn candidate(&self, email: &str) -> Result<Option<CandidateAt>, Error> {
        let mut result = self.inner.candidate(email).await?;
        if let Some(c) = &mut result {
            c.observed_ms -= self.age;
        }
        self.ready.notify_one();
        self.resume.notified().await;
        Ok(result)
    }
    async fn denied(&self, id: OperationId, request: &Self::Request) -> Result<(), Error> {
        self.inner.denied(id, request).await
    }
    async fn execute(&self, proof: Verified<Self::Request>) -> Result<Self::Outcome, Error> {
        self.inner.execute(proof).await
    }
}
#[tokio::test]
async fn a_page_waiting_for_the_primary_fence_rechecks_committed_demotion_and_proof_age() {
    for expired in [false, true] {
        let db = oidc::fixture().await;
        insert_principal(&db, 3, true).await;
        let held = Held {
            inner: db.store.operator_directory(),
            ready: tokio::sync::Notify::new(),
            resume: tokio::sync::Notify::new(),
            age: if expired { 60_000 } else { 0 },
        };
        let mut tx = db.pool.begin().await.unwrap();
        sqlx::query("SELECT singleton FROM security_state FOR UPDATE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let operation = list(&held, "one@example.com", query());
        let reduce = async {
            held.ready.notified().await;
            held.resume.notify_one();
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    let blocked: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE '%security_state%')").fetch_one(&db.pool).await.unwrap();
                    if blocked { break; }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            }).await.unwrap();
            if !expired {
                sqlx::query("DELETE FROM platform_administrators WHERE principal_id=$1")
                    .bind(Uuid::from_u128(1))
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            tx.commit().await.unwrap();
        };
        let (result, ()) = tokio::join!(operation, reduce);
        assert!(matches!(result, Err(Error::Denied)));
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM operator_directory_audit WHERE result='denied'"
            )
            .fetch_one(&db.pool)
            .await
            .unwrap(),
            1
        );
    }
}
#[tokio::test]
async fn directory_commit_failure_returns_uncertainty_without_a_page_or_retry() {
    let db = oidc::fixture().await;
    sqlx::raw_sql("CREATE FUNCTION reject_directory_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture'; END $$; CREATE CONSTRAINT TRIGGER reject_directory_commit AFTER INSERT ON operator_directory_audit DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_directory_commit();").execute(&db.pool).await.unwrap();
    assert!(matches!(
        list(&db.store.operator_directory(), "one@example.com", query()).await,
        Err(Error::Uncertain)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM operator_directory_audit")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn lost_directory_commit_reply_discloses_no_page_and_retains_one_audit() {
    let db = oidc::fixture().await;
    let (store, proxy) = super::limiter_activation::lost_nth_commit(&db.pool, 1).await;
    assert!(matches!(
        list(&store.operator_directory(), "one@example.com", query()).await,
        Err(Error::Uncertain)
    ));
    proxy.await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM operator_directory_audit WHERE result='read'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        1
    );
    store.close().await;
}
#[tokio::test]
async fn suppressed_directory_audit_cannot_release_a_page() {
    let db = oidc::fixture().await;
    sqlx::raw_sql("CREATE FUNCTION suppress_directory_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NULL; END $$; CREATE TRIGGER suppress_directory_audit BEFORE INSERT ON operator_directory_audit FOR EACH ROW EXECUTE FUNCTION suppress_directory_audit();").execute(&db.pool).await.unwrap();
    assert!(matches!(
        list(&db.store.operator_directory(), "one@example.com", query()).await,
        Err(Error::Unavailable)
    ));
}
