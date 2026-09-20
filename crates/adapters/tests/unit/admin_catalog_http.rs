use super::*;
#[test]
fn catalog_query_and_counter_profiles_are_bounded_and_canonical() {
    assert!(input::list("applications", Some("search=%25_&limit=100")).is_ok());
    for raw in [
        "limit=101",
        "skip=1",
        "search=a&search=b",
        "application_id=invalid",
    ] {
        assert!(input::list("applications", Some(raw)).is_err());
    }
    assert!(input::list("clients", None).is_err());
    assert!(input::target("capabilities", "00000000-0000-0000-0000-000000000001", None).is_ok());
    assert!(input::target("resources", "00000000-0000-0000-0000-000000000001", None).is_err());
}
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use darkhorse_application::registration;
use darkhorse_domain::admin_catalog::{Change, Query};
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::ServiceExt;
#[derive(Clone)]
struct Fake {
    calls: Arc<AtomicUsize>,
    failure: Option<Error>,
}
impl Fake {
    fn check(&self, actor: [u8; 32]) -> Result<(), Error> {
        assert_eq!(
            actor,
            crate::session_secret::digest(&"a".repeat(64)).unwrap()
        );
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.failure.map_or(Ok(()), Err)
    }
}
fn role() -> Item {
    Item::Role(RoleSummary {
        id: RoleId::from_u128(1).unwrap(),
        name: "Reader".into(),
    })
}
fn record() -> registration::Record {
    registration::Record::Application(registration::ApplicationRecord {
        id: ApplicationId::from_u128(1).unwrap(),
        name: "Portal".into(),
        owner: PrincipalId::from_u128(2).unwrap(),
        owner_email: "owner@example.com".into(),
        active: true,
        revision: 9007199254740993,
    })
}
impl Catalog for Fake {
    async fn list(&self, actor: [u8; 32], _: List, _: Query) -> Result<Page, Error> {
        self.check(actor)?;
        Ok(Page {
            items: vec![role()],
            next: None,
            policy_revision: 9007199254740993,
        })
    }
    async fn view(&self, actor: [u8; 32], _: Target) -> Result<View, Error> {
        self.check(actor)?;
        Ok(View {
            item: role(),
            applications: vec![],
            capabilities: vec![],
            policy_revision: 9007199254740993,
        })
    }
    async fn write(&self, actor: [u8; 32], revision: u64, _: Change) -> Result<Written, Error> {
        self.check(actor)?;
        assert_eq!(revision, 9007199254740993);
        Ok(Written {
            target: Target::Role(RoleId::from_u128(1).unwrap()),
            policy_revision: 9007199254740994,
        })
    }
}
impl Registration for Fake {
    async fn read(&self, actor: [u8; 32], _: ReadTarget) -> Result<registration::Record, Error> {
        self.check(actor)?;
        Ok(record())
    }
    async fn write(
        &self,
        actor: [u8; 32],
        _: registration::Command,
    ) -> Result<registration::Written, Error> {
        self.check(actor)?;
        Ok(registration::Written {
            record: record(),
            secret: None,
        })
    }
}
fn app(failure: Option<Error>) -> (Router, Arc<AtomicUsize>) {
    let fake = Fake {
        calls: Arc::default(),
        failure,
    };
    (
        crate::http::with_authentication(
            std::path::PathBuf::new(),
            router(
                fake.clone(),
                fake.clone(),
                url::Url::parse("https://localhost:8443").unwrap(),
            ),
        ),
        fake.calls,
    )
}
fn request(method: &str, path: &str) -> axum::http::request::Builder {
    Request::builder()
        .method(method)
        .uri(path)
        .header("host", "localhost:8443")
        .header("origin", "https://localhost:8443")
        .header("x-darkhorse-csrf", "1")
        .header("content-type", "application/json")
        .header("cookie", format!("__Host-darkhorse={}", "a".repeat(64)))
}
#[tokio::test]
async fn catalog_http_keeps_counters_exact_and_responses_noncacheable() {
    let (app, calls) = app(None);
    let reference = "00000000-0000-0000-0000-000000000001";
    for path in [
        "/api/admin/catalog/roles".into(),
        format!("/api/admin/catalog/roles/{reference}"),
        format!("/api/admin/console/applications/{reference}"),
        format!("/api/admin/console/applications/{reference}/clients/{reference}"),
    ] {
        let response = app
            .clone()
            .oneshot(request("GET", &path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let value: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 10000).await.unwrap()).unwrap();
        assert!(
            value["policy_revision"] == "9007199254740993"
                || value["revision"] == "9007199254740993"
        );
    }
    let response=app.clone().oneshot(request("POST","/api/admin/catalog").body(Body::from(json!({"policy_revision":"9007199254740993","change":{"operation":"create_role","name":"Reader"}}).to_string())).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 10000).await.unwrap()).unwrap();
    assert_eq!(value["policy_revision"], "9007199254740994");
    let response=app.oneshot(request("POST","/api/admin/console/registration").body(Body::from(json!({"operation":"create_application","application":{"name":"Portal","owner_id":reference,"active":true}}).to_string())).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(calls.load(Ordering::SeqCst), 6);
}
#[tokio::test]
async fn invalid_counters_unknown_fields_and_csrf_are_rejected_before_service_calls() {
    let (app, calls) = app(None);
    for revision in [
        json!("01"),
        json!("9223372036854775808"),
        json!(-1),
        json!("1e3"),
    ] {
        let response=app.clone().oneshot(request("POST","/api/admin/catalog").body(Body::from(json!({"policy_revision":revision,"change":{"operation":"create_role","name":"Reader"}}).to_string())).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    for path in [
        "/api/admin/catalog/roles?limit=101",
        "/api/admin/catalog/roles?search=x&search=y",
        "/api/admin/catalog/roles/not-a-uuid",
        "/api/admin/catalog/roles?application_id=bad",
    ] {
        assert_eq!(
            app.clone()
                .oneshot(request("GET", path).body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let mut req = request("POST", "/api/admin/catalog")
        .body(Body::from("{}"))
        .unwrap();
    req.headers_mut().remove("x-darkhorse-csrf");
    assert_eq!(
        app.oneshot(req).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn catalog_service_failures_keep_safe_transport_status() {
    for (error, status) in [
        (Error::Unauthorized, 401),
        (Error::Forbidden, 403),
        (Error::RecentAuthenticationRequired, 403),
        (Error::NotFound, 404),
        (Error::Conflict, 409),
        (Error::Invalid, 400),
        (Error::Unavailable, 503),
    ] {
        let (app, _) = app(Some(error));
        let response = app
            .oneshot(
                request("GET", "/api/admin/catalog/roles")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
        assert_eq!(response.headers()["cache-control"], "no-store");
    }
}
