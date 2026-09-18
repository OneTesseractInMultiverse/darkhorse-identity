use super::*;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tower::ServiceExt;
struct Fake {
    calls: Arc<AtomicUsize>,
    changes: Arc<Mutex<Vec<Change>>>,
    failure: Option<RegistrationError>,
}
impl Registry for Fake {
    async fn write(&self, actor: [u8; 32], command: Command) -> Result<Written, RegistrationError> {
        assert_eq!(
            actor,
            crate::session_secret::digest(&"a".repeat(64)).unwrap()
        );
        assert_eq!(command.target, record().target);
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.changes.lock().unwrap().push(command.change);
        if let Some(e) = self.failure {
            return Err(e);
        }
        Ok(Written {
            record: record(),
            secret: command.change.needs_secret().then(|| "reveal-once".into()),
        })
    }
    async fn read(&self, _: [u8; 32], target: Target) -> Result<Record, RegistrationError> {
        assert_eq!(target, record().target);
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.failure.map_or(Ok(record()), Err)
    }
}
fn record() -> Record {
    Record {
        target: Target {
            application: ApplicationId::from_u128(1).unwrap(),
            resource: ResourceId::from_u128(2).unwrap(),
        },
        active: true,
        revision: 0,
        secrets: vec![SecretMetadata {
            id: CredentialId::from_u128(3).unwrap(),
            created_ms: 10,
            expires_ms: None,
        }],
    }
}
fn app(failure: Option<RegistrationError>) -> (Router, Arc<AtomicUsize>, Arc<Mutex<Vec<Change>>>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let changes = Arc::new(Mutex::new(vec![]));
    let routes = router(
        Fake {
            calls: calls.clone(),
            changes: changes.clone(),
            failure,
        },
        url::Url::parse("https://localhost:8443").unwrap(),
    );
    (
        crate::http::with_authentication(std::path::PathBuf::new(), routes),
        calls,
        changes,
    )
}
const WRITE: &str = "/api/admin/resource-introspection";
const READ: &str = "/api/admin/applications/00000000-0000-0000-0000-000000000001/resources/00000000-0000-0000-0000-000000000002/introspection";
fn input() -> Value {
    json!({"operation":"register","application_id":"00000000-0000-0000-0000-000000000001","resource_id":"00000000-0000-0000-0000-000000000002"})
}
fn request(method: &str, path: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("host", "localhost:8443")
        .header("origin", "https://localhost:8443")
        .header("x-darkhorse-csrf", "1")
        .header("content-type", "application/json")
        .header("cookie", format!("__Host-darkhorse={}", "a".repeat(64)))
        .body(if method == "GET" {
            Body::empty()
        } else {
            Body::from(body.to_string())
        })
        .unwrap()
}
async fn value(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap()
}
#[tokio::test]
async fn exact_commands_return_one_time_secrets_and_metadata_reads_are_redacted() {
    let (app, _, changes) = app(None);
    for (operation, change) in [
        ("register", Change::Register),
        (
            "rotate",
            Change::Rotate {
                revision: 4,
                overlap_seconds: 30,
            },
        ),
        (
            "set_active",
            Change::SetActive {
                revision: 4,
                active: false,
            },
        ),
    ] {
        let mut body = input();
        body["operation"] = json!(operation);
        if operation != "register" {
            body["revision"] = json!(4);
        }
        if operation == "rotate" {
            body["overlap_seconds"] = json!(30);
        }
        if operation == "set_active" {
            body["active"] = json!(false);
        }
        let response = app
            .clone()
            .oneshot(request("POST", WRITE, body))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert!(
            response.headers()["cache-control"]
                .to_str()
                .unwrap()
                .contains("no-store")
        );
        let response = value(response).await;
        assert_eq!(response.get("secret").is_some(), change.needs_secret());
        assert_eq!(
            response["introspection_client_id"],
            "rs_00000000-0000-0000-0000-000000000002"
        );
        assert_eq!(*changes.lock().unwrap().last().unwrap(), change);
    }
    let response = app
        .oneshot(request("GET", READ, Value::Null))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let response = value(response).await;
    assert!(response.get("secret").is_none());
    assert!(!response.to_string().contains("verifier"));
    assert_eq!(
        response["secrets"],
        json!([{"id":"00000000-0000-0000-0000-000000000003","created_ms":10,"expires_ms":null}])
    );
}
#[tokio::test]
async fn malformed_inputs_missing_actor_and_cross_site_writes_never_reach_service() {
    let (app, calls, _) = app(None);
    for (key, val) in [
        ("actor_id", json!("spoof")),
        ("application_id", json!("invalid")),
        ("resource_id", json!(uuid::Uuid::nil().to_string())),
        ("resource_id", json!("00000000000000000000000000000002")),
        ("operation", json!("unknown")),
    ] {
        let mut body = input();
        body[key] = val;
        assert_eq!(
            app.clone()
                .oneshot(request("POST", WRITE, body))
                .await
                .unwrap()
                .status(),
            400
        );
    }
    for (header, replacement, status) in [
        ("cookie", None, 401),
        ("cookie", Some("__Host-darkhorse=invalid"), 401),
        ("origin", Some("https://hostile.example"), 403),
        ("x-darkhorse-csrf", None, 403),
    ] {
        let mut req = request("POST", WRITE, input());
        req.headers_mut().remove(header);
        if let Some(replacement) = replacement {
            req.headers_mut()
                .insert(header, replacement.parse().unwrap());
        }
        assert_eq!(app.clone().oneshot(req).await.unwrap().status(), status);
    }
    let mut req = request("POST", WRITE, input());
    *req.body_mut() = Body::from("x".repeat(2049));
    assert_eq!(app.clone().oneshot(req).await.unwrap().status(), 413);
    let mut req = request("GET", READ, Value::Null);
    req.headers_mut().remove("cookie");
    assert_eq!(app.clone().oneshot(req).await.unwrap().status(), 401);
    assert_eq!(
        app.oneshot(request(
            "GET",
            &READ.replace("000000000002", "000000000000"),
            Value::Null
        ))
        .await
        .unwrap()
        .status(),
        400
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn authority_and_storage_failures_never_disclose_credentials() {
    for (failure, status) in [
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
        for (method, path) in [("POST", WRITE), ("GET", READ)] {
            let response = app
                .clone()
                .oneshot(request(method, path, input()))
                .await
                .unwrap();
            assert_eq!(response.status(), status);
            let response = value(response).await;
            assert!(response.get("secret").is_none());
            assert!(response.get("secrets").is_none());
        }
    }
}
