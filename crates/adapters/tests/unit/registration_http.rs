use super::*;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tower::ServiceExt;
struct Fake {
    calls: Arc<AtomicUsize>,
    commands: Arc<Mutex<Vec<Command>>>,
    failure: Option<RegistrationError>,
}
impl Registration for Fake {
    async fn write(&self, actor: [u8; 32], command: Command) -> Result<Written, RegistrationError> {
        assert_eq!(
            actor,
            crate::session_secret::digest(&"a".repeat(64)).unwrap()
        );
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.commands.lock().unwrap().push(command);
        if let Some(e) = self.failure {
            return Err(e);
        }
        Ok(Written {
            record: record(),
            secret: Some("reveal-once".into()),
        })
    }
    async fn read(&self, _: [u8; 32], _: ReadTarget) -> Result<Record, RegistrationError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.failure.map_or(Ok(record()), Err)
    }
}
fn record() -> Record {
    Record::Application(ApplicationRecord {
        id: ApplicationId::from_u128(1).unwrap(),
        name: "Portal".into(),
        owner: PrincipalId::from_u128(2).unwrap(),
        owner_email: "owner@example.com".into(),
        active: true,
        revision: 0,
    })
}
fn app(failure: Option<RegistrationError>) -> (Router, Arc<AtomicUsize>, Arc<Mutex<Vec<Command>>>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let commands = Arc::new(Mutex::new(vec![]));
    let router = router(
        Fake {
            calls: calls.clone(),
            commands: commands.clone(),
            failure,
        },
        url::Url::parse("https://localhost:8443").unwrap(),
    );
    (
        crate::http::with_authentication(std::path::PathBuf::new(), router),
        calls,
        commands,
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
fn id(n: u128) -> String {
    uuid::Uuid::from_u128(n).to_string()
}
fn create() -> Value {
    json!({"operation":"create_application","application":{"name":"Portal","owner_id":id(2),"active":true}})
}
fn client() -> Value {
    json!({"name":"Web","active":true,"redirect_uris":["https://app.example/cb"],"resource_ids":[id(3)],"scope_ids":[id(4)],"token_endpoint_auth_method":"client_secret_basic"})
}
#[test]
fn refresh_issuance_requires_explicit_client_opt_in() {
    for (setting, expected) in [(None, false), (Some(false), false), (Some(true), true)] {
        let mut value = client();
        if let Some(setting) = setting {
            value["refresh_tokens"] = setting.into();
        }
        let input: input::Input = serde_json::from_value(
            json!({"operation":"create_client", "application_id":id(1), "client":value}),
        )
        .unwrap();
        let Command::CreateClient { spec, .. } = input.command().unwrap() else {
            panic!("client command")
        };
        assert_eq!(spec.refresh_tokens, expected);
    }
}
async fn post(app: Router, value: Value) -> axum::response::Response {
    app.oneshot(
        request("POST", "/api/admin/registration")
            .body(Body::from(value.to_string()))
            .unwrap(),
    )
    .await
    .unwrap()
}
#[tokio::test]
async fn registration_transport_passes_exact_commands_and_returns_secrets_only_on_successful_write()
{
    let (app, calls, commands) = app(None);
    let inputs = [
        create(),
        json!({"operation":"update_application","application_id":id(1),"revision":1,"application":{"name":"New","owner_id":id(2),"active":false}}),
        json!({"operation":"create_resource","application_id":id(1),"name":"API"}),
        json!({"operation":"create_scope","application_id":id(1),"resource_id":id(3),"name":"read"}),
        json!({"operation":"create_client","application_id":id(1),"client":client()}),
        json!({"operation":"update_client","application_id":id(1),"client_id":id(5),"revision":2,"client":client()}),
        json!({"operation":"rotate_secret","application_id":id(1),"client_id":id(5),"revision":3,"overlap_seconds":30}),
        json!({"operation":"retire_secret","application_id":id(1),"client_id":id(5),"secret_id":id(6),"revision":4}),
    ];
    for input in inputs {
        let response = post(app.clone(), input).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()["cache-control"]
                .to_str()
                .unwrap()
                .contains("no-store")
        );
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["client_secret"], "reveal-once");
        assert!(!String::from_utf8_lossy(&bytes).contains("verifier"));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 8);
    {
        let commands = commands.lock().unwrap();
        assert!(
            matches!(&commands[4],Command::CreateClient{application,spec} if application.as_u128()==1 && spec.resources[0].as_u128()==3 && spec.scopes[0].as_u128()==4 && spec.redirects.allows("https://app.example/cb"))
        );
    }
    for path in [
        format!("/api/admin/applications/{}", id(1)),
        format!("/api/admin/applications/{}/clients/{}", id(1), id(5)),
    ] {
        let response = app
            .clone()
            .oneshot(request("GET", &path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("client_secret"));
    }
}
#[tokio::test]
async fn malformed_or_ambiguous_requests_do_not_reach_service() {
    let (app, calls, _) = app(None);
    let mut invalid_client = client();
    invalid_client["token_endpoint_auth_method"] = json!("client_secret_jwt");
    let mut wildcard = client();
    wildcard["redirect_uris"] = json!(["https://*.example/cb"]);
    let mut unknown = create();
    unknown["actor_id"] = json!(id(1));
    for input in [
        json!({}),
        unknown,
        json!({"operation":"create_client","application_id":id(1),"client":invalid_client}),
        json!({"operation":"create_client","application_id":id(1),"client":wildcard}),
        json!({"operation":"create_resource","application_id":"not-a-uuid","name":"API"}),
        json!({"operation":"create_resource","application_id":id(0),"name":"API"}),
        json!({"operation":"create_resource","application_id":"00000000000000000000000000000001","name":"API"}),
        json!({"operation":"create_scope","application_id":id(1),"resource_id":id(3),"name":"openid"}),
    ] {
        assert_eq!(
            post(app.clone(), input).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
    let mut missing = request("POST", "/api/admin/registration")
        .body(Body::from(create().to_string()))
        .unwrap();
    missing.headers_mut().remove("cookie");
    assert_eq!(
        app.clone().oneshot(missing).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let duplicate = request("POST", "/api/admin/registration")
        .header("cookie", format!("__Host-darkhorse={}", "b".repeat(64)))
        .body(Body::from(create().to_string()))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(duplicate).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let mut csrf = request("POST", "/api/admin/registration")
        .body(Body::from(create().to_string()))
        .unwrap();
    csrf.headers_mut().remove("x-darkhorse-csrf");
    assert_eq!(
        app.clone().oneshot(csrf).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let oversized = request("POST", "/api/admin/registration")
        .body(Body::from("x".repeat(32769)))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(oversized).await.unwrap().status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    for path in [
        "/api/admin/applications/invalid",
        "/api/admin/applications/invalid/clients/invalid",
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
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn anonymous_reads_never_load_registration_metadata() {
    let (app, calls, _) = app(None);
    for path in [
        format!("/api/admin/applications/{}", id(1)),
        format!("/api/admin/applications/{}/clients/{}", id(1), id(5)),
    ] {
        let mut request = request("GET", &path).body(Body::empty()).unwrap();
        request.headers_mut().remove("cookie");
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn denied_and_failed_writes_are_redacted_and_never_contain_a_secret() {
    for (failure, status) in [
        (RegistrationError::Invalid, StatusCode::BAD_REQUEST),
        (RegistrationError::Unauthorized, StatusCode::UNAUTHORIZED),
        (RegistrationError::Forbidden, StatusCode::FORBIDDEN),
        (
            RegistrationError::RecentAuthenticationRequired,
            StatusCode::FORBIDDEN,
        ),
        (RegistrationError::NotFound, StatusCode::NOT_FOUND),
        (RegistrationError::Conflict, StatusCode::CONFLICT),
        (
            RegistrationError::Unavailable,
            StatusCode::SERVICE_UNAVAILABLE,
        ),
    ] {
        let (app, _, _) = app(Some(failure));
        let response = post(app.clone(), create()).await;
        assert_eq!(response.status(), status);
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("client_secret"));
        assert!(!String::from_utf8_lossy(&bytes).contains("owner@example.com"));
        let response = app
            .oneshot(
                request("GET", &format!("/api/admin/applications/{}", id(1)))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
    }
}
