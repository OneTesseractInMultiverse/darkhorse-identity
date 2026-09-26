use super::*;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use tower::ServiceExt;
#[tokio::test]
async fn public_presentation_discloses_only_the_allowlisted_default_without_authentication() {
    for (locale, tag) in [(Locale::English, "en"), (Locale::Spanish, "es")] {
        let app = router(locale, url::Url::parse("https://identity.example").unwrap());
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/presentation")
                    .header("host", "identity.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert!(response.headers().get("set-cookie").is_none());
        let json: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
        assert_eq!(json, serde_json::json!({"default_locale":tag}));
        assert_eq!(
            app.oneshot(
                Request::builder()
                    .uri("/api/presentation?email=untrusted")
                    .header("host", "identity.example")
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
            403
        );
    }
}
