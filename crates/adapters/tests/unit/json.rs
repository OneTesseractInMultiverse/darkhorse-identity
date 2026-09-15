use super::*;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::Request,
    routing::post,
};
use serde::Deserialize;
use tower::ServiceExt;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Input {
    name: String,
}

async fn echo(SafeJson(input): SafeJson<Input>) -> Json<Input> {
    Json(input)
}

async fn submit(body: &str, content_type: &str) -> axum::response::Response {
    Router::new()
        .route("/", post(echo))
        .layer(axum::extract::DefaultBodyLimit::max(64))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", content_type)
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn accepts_valid_transport_data() {
    let response = submit(r#"{"name":"Alex"}"#, "application/json").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(response.into_body(), 1024).await.unwrap(),
        r#"{"name":"Alex"}"#
    );
}

#[tokio::test]
async fn malformed_data_is_redacted_with_accurate_status() {
    for (body, content_type, status) in [
        (r#"{"name":12}"#, "application/json", 400),
        (r#"{"secret":"private"}"#, "application/json", 400),
        ("broken", "application/json", 400),
        ("anything", "text/plain", 415),
        (&"x".repeat(65), "application/json", 413),
    ] {
        let response = submit(body, content_type).await;
        assert_eq!(response.status().as_u16(), status);
        assert_eq!(
            to_bytes(response.into_body(), 1024).await.unwrap(),
            r#"{"error":"invalid_request"}"#
        );
    }
}
