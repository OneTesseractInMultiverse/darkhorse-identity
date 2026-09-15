use super::*;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt;

#[tokio::test]
async fn liveness_has_explicit_transport_and_security_headers() {
    let response = router("unused-unit-test-path".into())
        .oneshot(
            Request::builder()
                .uri("/health/live")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/json");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(
        to_bytes(response.into_body(), 1024).await.unwrap(),
        r#"{"status":"ok"}"#
    );
}

#[tokio::test]
async fn unknown_api_and_protocol_routes_never_use_frontend_fallback() {
    for path in [
        "/api",
        "/api/missing",
        "/oauth/token",
        "/.well-known/openid-configuration",
        "/health/ready",
        "/unexpected",
    ] {
        let response = router("unused-unit-test-path".into())
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        assert_eq!(
            to_bytes(response.into_body(), 1024).await.unwrap(),
            r#"{"error":"not_found"}"#
        );
    }
}

#[tokio::test]
async fn liveness_rejects_writes() {
    let response = router("unused-unit-test-path".into())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/health/live")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}
