use super::*;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tower::ServiceExt;
struct Fake {
    calls: Arc<AtomicUsize>,
    failure: Option<Error>,
}
impl SessionManagement for Fake {
    async fn sessions(&self, _: [u8; 32], after: Option<Cursor>) -> Result<Page, Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = self.failure {
            return Err(error);
        }
        Ok(Page {
            current: id("00000000-0000-0000-0000-000000000001")?,
            items: vec![],
            next: after,
        })
    }
    async fn end_session(&self, _: [u8; 32], target: SessionId) -> Result<Ended, Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.failure.map_or(
            Ok(Ended {
                current: target.as_u128() == 1,
            }),
            Err,
        )
    }
}
fn app(failure: Option<Error>) -> (Router, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    (
        router(
            Fake {
                calls: calls.clone(),
                failure,
            },
            url::Url::parse("https://localhost:8443").unwrap(),
        ),
        calls,
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
#[test]
fn cursor_validation_rejects_unbounded_ambiguous_or_noncanonical_inputs() {
    assert_eq!(cursor(None), Ok(None));
    for value in [
        "",
        "after=",
        "after=1:0",
        "after=01:00000000-0000-0000-0000-000000000001",
        "after=9223372036854775808:00000000-0000-0000-0000-000000000001",
        "after=1:00000000-0000-0000-0000-000000000001&extra=x",
        "extra=x",
        "after=1:00000000-0000-0000-0000-000000000000",
    ] {
        assert_eq!(cursor(Some(value)), Err(Error::Invalid));
    }
    assert_eq!(cursor(Some(&"x".repeat(129))), Err(Error::Invalid));
    let expected = Cursor::new(1, id("00000000-0000-0000-0000-000000000001").unwrap()).unwrap();
    assert_eq!(
        cursor(Some("after=1:00000000-0000-0000-0000-000000000001")),
        Ok(Some(expected))
    );
}
#[tokio::test]
async fn session_transport_projects_only_public_references_and_clears_only_ended_actor_cookie() {
    let (app, calls) = app(None);
    let response = app
        .clone()
        .oneshot(
            request("GET", "/api/security/sessions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"current":"00000000-0000-0000-0000-000000000001","items":[],"next":null})
    );
    for n in [2, 1] {
        let response = app
            .clone()
            .oneshot(
                request("POST", "/api/security/sessions/end")
                    .body(Body::from(format!(
                        "{{\"session_id\":\"{}\"}}",
                        uuid::Uuid::from_u128(n)
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers().contains_key("set-cookie"), n == 1);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}
#[tokio::test]
async fn invalid_inputs_and_browser_boundaries_do_not_reach_session_management() {
    let (app, calls) = app(None);
    for value in [
        "{}",
        "{\"session_id\":\"not-a-uuid\"}",
        "{\"session_id\":\"00000000-0000-0000-0000-000000000001\",\"principal_id\":\"foreign\"}",
    ] {
        let response = app
            .clone()
            .oneshot(
                request("POST", "/api/security/sessions/end")
                    .body(Body::from(value))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }
    for (method, path, expected) in [
        ("GET", "/api/security/sessions?owner=foreign", 400),
        ("POST", "/api/security/sessions/end?x=1", 403),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(request(method, path).body(Body::from("{}")).unwrap())
                .await
                .unwrap()
                .status(),
            expected
        );
    }
    let mut req = request("POST", "/api/security/sessions/end")
        .body(Body::from("{}"))
        .unwrap();
    req.headers_mut().remove("x-darkhorse-csrf");
    assert_eq!(app.clone().oneshot(req).await.unwrap().status(), 403);
    let mut req = request("GET", "/api/security/sessions")
        .body(Body::empty())
        .unwrap();
    req.headers_mut().remove("cookie");
    assert_eq!(app.oneshot(req).await.unwrap().status(), 401);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn session_errors_are_fixed_and_authentication_failures_clear_the_cookie() {
    for (error, status) in [
        (Error::Unauthorized, 401),
        (Error::NotFound, 404),
        (Error::Unavailable, 503),
        (Error::Invalid, 400),
    ] {
        let (app, _) = app(Some(error));
        let response = app
            .oneshot(
                request("GET", "/api/security/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(response.headers().contains_key("set-cookie"), status == 401);
    }
}

#[test]
fn session_projection_keeps_only_public_ids_timestamps_and_status() {
    use darkhorse_domain::sessions::Record;
    let current = SessionId::from_u128(1).unwrap();
    let other = SessionId::from_u128(2).unwrap();
    let value = projection(Page {
        current,
        items: vec![
            Record {
                id: current,
                created_ms: 10,
                seen_ms: 20,
                expires_ms: 30,
                status: Status::Active,
            },
            Record {
                id: other,
                created_ms: 1,
                seen_ms: 2,
                expires_ms: 3,
                status: Status::Inactive,
            },
        ],
        next: Some(Cursor::new(1, other).unwrap()),
    });
    assert_eq!(
        value,
        json!({"current":"00000000-0000-0000-0000-000000000001","items":[{"id":"00000000-0000-0000-0000-000000000001","created_ms":10,"seen_ms":20,"expires_ms":30,"status":"active"},{"id":"00000000-0000-0000-0000-000000000002","created_ms":1,"seen_ms":2,"expires_ms":3,"status":"inactive"}],"next":"1:00000000-0000-0000-0000-000000000002"})
    );
}
#[tokio::test]
async fn foreign_origins_duplicate_cookies_and_public_ids_cannot_authorize_changes() {
    let (app, calls) = app(None);
    for (header, value, status) in [
        ("origin", "https://other.example", 403),
        (
            "cookie",
            "__Host-darkhorse=00000000-0000-0000-0000-000000000001",
            401,
        ),
        (
            "cookie",
            &format!("__Host-darkhorse={0}; __Host-darkhorse={0}", "a".repeat(64)),
            401,
        ),
    ] {
        let mut req = request("POST", "/api/security/sessions/end")
            .body(Body::from(
                "{\"session_id\":\"00000000-0000-0000-0000-000000000001\"}",
            ))
            .unwrap();
        req.headers_mut().insert(
            axum::http::header::HeaderName::from_bytes(header.as_bytes()).unwrap(),
            value.parse().unwrap(),
        );
        assert_eq!(app.clone().oneshot(req).await.unwrap().status(), status);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
