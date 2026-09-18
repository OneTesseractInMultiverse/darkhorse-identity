use super::*;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use darkhorse_application::authentication::SessionSecret;
use darkhorse_domain::identity::PrincipalId;
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::ServiceExt;
struct Fake {
    calls: Arc<AtomicUsize>,
    result: Option<AuthError>,
}
impl BrowserAuthentication for Fake {
    async fn login(
        &self,
        email: &str,
        password: &str,
        _: Option<[u8; 32]>,
    ) -> Result<SignedIn, AuthError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(email, "a@example.com");
        assert_eq!(password, "test-only password");
        if let Some(e) = self.result {
            return Err(e);
        }
        Ok(SignedIn {
            secret: SessionSecret {
                value: "a".repeat(64),
                digest: [1; 32],
            },
            view: view(),
        })
    }
    async fn session(&self, _: [u8; 32]) -> Result<SessionView, AuthError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.result.map_or(Ok(view()), Err)
    }
    async fn logout(&self, _: [u8; 32]) -> Result<(), AuthError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.result.map_or(Ok(()), Err)
    }
}
fn view() -> SessionView {
    SessionView {
        principal: PrincipalId::from_u128(1).unwrap(),
        name: "Ada".into(),
    }
}
fn app(result: Option<AuthError>) -> (Router, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    (
        router(
            Fake {
                calls: calls.clone(),
                result,
            },
            url::Url::parse("https://localhost:8443").unwrap(),
        ),
        calls,
    )
}
fn request(path: &str, method: &str) -> axum::http::request::Builder {
    Request::builder()
        .method(method)
        .uri(path)
        .header("host", "localhost:8443")
        .header("origin", "https://localhost:8443")
        .header("x-darkhorse-csrf", "1")
        .header("content-type", "application/json")
}
fn body() -> Body {
    Body::from(r#"{"email":"a@example.com","password":"test-only password"}"#)
}

#[test]
fn every_unsafe_method_requires_origin_and_csrf() {
    let boundary = Boundary {
        origin: "https://localhost:8443".into(),
        host: "localhost:8443".into(),
        queries: false,
        service: false,
        slots: Semaphore::new(1),
    };
    let mut headers = HeaderMap::new();
    headers.insert("host", HeaderValue::from_static("localhost:8443"));
    for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
        assert!(!allowed(&boundary, &headers, &method, None));
    }
}

#[tokio::test]
async fn disabled_authentication_never_claims_login_or_logout_success() {
    for (path, method) in [
        ("/api/auth/login", "POST"),
        ("/api/auth/session", "GET"),
        ("/api/auth/logout", "POST"),
    ] {
        let response = disabled_router()
            .oneshot(request(path, method).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(response.headers().get("set-cookie").is_none());
    }
}

#[tokio::test]
async fn exhausted_http_admission_never_reads_a_body_or_invokes_service() {
    let boundary = Arc::new(Boundary {
        origin: "https://localhost:8443".into(),
        host: "localhost:8443".into(),
        queries: false,
        service: false,
        slots: Semaphore::new(0),
    });
    let (inner, calls) = app(None);
    let app = inner.layer(middleware::from_fn_with_state(boundary, guard));
    let response = app
        .oneshot(request("/api/auth/login", "POST").body(body()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn issues_host_only_http_only_cookie_and_returns_only_public_profile() {
    let (app, calls) = app(None);
    let response = app
        .oneshot(request("/api/auth/login", "POST").body(body()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response.headers()["set-cookie"].to_str().unwrap();
    assert!(cookie.starts_with("__Host-darkhorse="));
    assert!(cookie.ends_with("; Path=/; Secure; HttpOnly; SameSite=Lax"));
    assert!(!cookie.contains("Domain"));
    assert_eq!(
        to_bytes(response.into_body(), 4096).await.unwrap(),
        r#"{"name":"Ada"}"#
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn untrusted_origins_duplicate_headers_queries_and_forwarded_spoofing_never_reach_service() {
    for (key, value) in [
        ("host", "evil.example"),
        ("origin", "https://evil.example"),
        ("x-darkhorse-csrf", "bad"),
        ("sec-fetch-site", "cross-site"),
    ] {
        let (app, calls) = app(None);
        let response = app
            .oneshot(
                request("/api/auth/login", "POST")
                    .header(key, value)
                    .body(body())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
    for path in ["/api/auth/login?token=secret", "/api/auth/login"] {
        let (app, calls) = app(None);
        let response = app
            .oneshot(
                Request::builder()
                    .uri(path)
                    .method("POST")
                    .header("host", "evil.example")
                    .header("forwarded", "host=localhost:8443;proto=https")
                    .header("x-forwarded-host", "localhost:8443")
                    .body(body())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
#[tokio::test]
async fn missing_csrf_invalid_json_oversized_body_and_cookie_ambiguity_are_rejected() {
    let (app, calls) = app(None);
    let mut missing = request("/api/auth/login", "POST").body(body()).unwrap();
    missing.headers_mut().remove("x-darkhorse-csrf");
    assert_eq!(
        app.clone().oneshot(missing).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    for (body, status) in [
        ("{}".into(), StatusCode::BAD_REQUEST),
        ("x".repeat(4097), StatusCode::PAYLOAD_TOO_LARGE),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(
                    request("/api/auth/login", "POST")
                        .body(Body::from(body))
                        .unwrap()
                )
                .await
                .unwrap()
                .status(),
            status
        );
    }
    for cookie in [
        "__Host-darkhorse=short".into(),
        format!(
            "__Host-darkhorse={}; __Host-darkhorse={}",
            "a".repeat(64),
            "b".repeat(64)
        ),
        "malformed".into(),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(
                    request("/api/auth/login", "POST")
                        .header("cookie", cookie)
                        .body(body())
                        .unwrap()
                )
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn session_logout_and_dependency_outcomes_have_explicit_transport_contracts() {
    let cookie = format!("__Host-darkhorse={}", "a".repeat(64));
    for (outcome, status) in [
        (None, StatusCode::OK),
        (Some(AuthError::Denied), StatusCode::UNAUTHORIZED),
        (
            Some(AuthError::Unavailable),
            StatusCode::SERVICE_UNAVAILABLE,
        ),
        (
            Some(AuthError::Limited {
                retry_after_ms: 1001,
            }),
            StatusCode::TOO_MANY_REQUESTS,
        ),
    ] {
        let (app, _) = app(outcome);
        let response = app
            .clone()
            .oneshot(request("/api/auth/login", "POST").body(body()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        if status == StatusCode::TOO_MANY_REQUESTS {
            assert_eq!(response.headers()["retry-after"], "2");
        }
        let response = app
            .clone()
            .oneshot(
                request("/api/auth/session", "GET")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        let response = app
            .oneshot(
                request("/api/auth/logout", "POST")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if outcome.is_none() {
                StatusCode::NO_CONTENT
            } else {
                status
            }
        );
    }
    let (app, calls) = app(None);
    assert_eq!(
        app.clone()
            .oneshot(
                request("/api/auth/session", "GET")
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let response = app
        .oneshot(
            request("/api/auth/logout", "POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(
        response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn confidential_token_requests_require_a_canonical_host_and_no_browser_credentials() {
    let boundary = Boundary {
        origin: "https://localhost:8443".into(),
        host: "localhost:8443".into(),
        queries: false,
        service: true,
        slots: Semaphore::new(1),
    };
    let mut headers = HeaderMap::new();
    headers.insert("host", HeaderValue::from_static("localhost:8443"));
    assert!(allowed(&boundary, &headers, &Method::POST, None));
    assert!(!allowed(
        &boundary,
        &headers,
        &Method::POST,
        Some("secret=untrusted")
    ));
    for (name, value) in [
        ("origin", "https://localhost:8443"),
        ("cookie", "unrelated=1"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
        assert!(!allowed(&boundary, &headers, &Method::POST, None));
        headers.remove(name);
    }
}
