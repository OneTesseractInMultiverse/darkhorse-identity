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

#[test]
fn user_role_assignments_remain_on_the_dedicated_directory_api() {
    let mutation = serde_json::json!({
        "policy_revision": "1",
        "change": {
            "operation": "principal_role",
            "principal_id": "00000000-0000-0000-0000-000000000001",
            "application_id": "00000000-0000-0000-0000-000000000002",
            "role_id": "00000000-0000-0000-0000-000000000003",
            "assigned": true,
            "principal_revision": "0"
        }
    });
    assert!(serde_json::from_value::<input::Mutation>(mutation).is_err());
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
    policy_map_failure: Option<PolicyMapError>,
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
    async fn policy_map(
        &self,
        actor: [u8; 32],
        application: ApplicationId,
    ) -> Result<darkhorse_domain::policy_map::Graph, PolicyMapError> {
        self.check(actor).map_err(|error| match error {
            Error::Unauthorized => PolicyMapError::Unauthorized,
            Error::Forbidden | Error::RecentAuthenticationRequired => PolicyMapError::Forbidden,
            Error::NotFound => PolicyMapError::NotFound,
            Error::Invalid | Error::Conflict | Error::Unavailable => PolicyMapError::Unavailable,
        })?;
        if let Some(error) = self.policy_map_failure {
            return Err(error);
        }
        Ok(darkhorse_domain::policy_map::Graph {
            application: darkhorse_domain::policy_map::Application {
                id: application,
                name: darkhorse_domain::registration::Label::new("Portal").unwrap(),
                active: true,
            },
            policy_revision: 9007199254740993,
            roles: vec![],
            capabilities: vec![darkhorse_domain::policy_map::Capability {
                id: CapabilityId::from_u128(3).unwrap(),
                definition: darkhorse_domain::admin_catalog::PermissionDefinition::new(
                    "records.read",
                    "Read organization records",
                )
                .unwrap(),
                retired: false,
            }],
            resources: vec![],
            scopes: vec![],
            edges: vec![],
        })
    }
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
            principal_revision: None,
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
        policy_map_failure: None,
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
fn policy_app(failure: Option<PolicyMapError>) -> (Router, Arc<AtomicUsize>) {
    let fake = Fake {
        calls: Arc::default(),
        failure: None,
        policy_map_failure: failure,
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
async fn policy_map_http_preserves_exact_revision_and_rejects_query_parameters() {
    let (app, calls) = app(None);
    let path = "/api/admin/console/applications/00000000-0000-0000-0000-000000000001/policy-map";
    let response = app
        .clone()
        .oneshot(request("GET", path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 10000).await.unwrap()).unwrap();
    assert_eq!(value["policy_revision"], "9007199254740993");
    assert_eq!(
        value["application"]["id"],
        "00000000-0000-0000-0000-000000000001"
    );
    assert_eq!(
        value["nodes"][0]["id"],
        "application:00000000-0000-0000-0000-000000000001"
    );
    assert_eq!(value["complete"], true);
    assert_eq!(value["nodes"][1]["type"], "capability");
    assert_eq!(value["nodes"][1]["name"], "records.read");
    assert_eq!(value["nodes"][1]["key"], "records.read");

    for invalid_path in [
        "/api/admin/console/applications/not-a-uuid/policy-map",
        "/api/admin/console/applications/00000000-0000-0000-0000-000000000001/policy-map?unknown=value",
    ] {
        let response = app
            .clone()
            .oneshot(request("GET", invalid_path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn oversized_policy_map_is_distinguished_from_an_unavailable_read() {
    for (error, status, code) in [
        (
            PolicyMapError::TooLarge,
            StatusCode::PAYLOAD_TOO_LARGE,
            "policy_map_too_large",
        ),
        (
            PolicyMapError::Unavailable,
            StatusCode::SERVICE_UNAVAILABLE,
            "temporarily_unavailable",
        ),
    ] {
        let (app, _) = policy_app(Some(error));
        let response = app
            .oneshot(
                request(
                    "GET",
                    "/api/admin/console/applications/00000000-0000-0000-0000-000000000001/policy-map",
                )
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        let value: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 10000).await.unwrap()).unwrap();
        assert_eq!(value["error"], code);
    }
}
#[tokio::test]
async fn serialized_policy_map_byte_limit_returns_only_a_no_store_error() {
    let response =
        policy_map_response(json!({"nodes":["x".repeat(MAX_POLICY_MAP_RESPONSE_BYTES)]}));
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 10000).await.unwrap()).unwrap();
    assert_eq!(value, json!({"error":"policy_map_too_large"}));
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

#[tokio::test]
async fn query_value_limit_rejections_are_redacted_before_service_calls() {
    let (app, calls) = app(None);
    let path = format!(
        "/api/admin/catalog/applications?limit={}25",
        "%30".repeat(30)
    );
    let response = app
        .clone()
        .oneshot(request("GET", &path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    for query in [
        format!("limit={}25", "%30".repeat(31)),
        "status=private%3E%3Dvalue".into(),
        "private%0Afield=value".into(),
        "status=%00private".into(),
        "status=/private/".into(),
    ] {
        let path = format!("/api/admin/catalog/applications?{query}");
        let response = app
            .clone()
            .oneshot(request("GET", &path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            json!({"error":"invalid_registration"})
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
