use super::*;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::ServiceExt;
struct Store(Arc<AtomicUsize>);
impl Registration for Store {
    async fn logout_configuration(
        &self,
        _: [u8; 32],
        _: Target,
    ) -> Result<Record, RegistrationError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(Record {
            configuration: Configuration {
                revision: 0,
                uri: None,
            },
            deliveries: vec![],
        })
    }
    async fn configure_logout(
        &self,
        _: [u8; 32],
        _: Target,
        value: Configuration,
    ) -> Result<Configuration, RegistrationError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(value)
    }
}
fn request() -> axum::http::request::Builder {
    Request::builder().method("PUT").uri("/api/admin/applications/00000000-0000-0000-0000-000000000001/clients/00000000-0000-0000-0000-000000000002/logout")
        .header("host","localhost:8443").header("content-type","application/json")
        .header("cookie",format!("__Host-darkhorse={}","a".repeat(64)))
}
#[tokio::test]
async fn logout_registration_requires_origin_csrf_cookie_and_session_specific_https_metadata() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = crate::http::with_authentication(
        std::path::PathBuf::new(),
        router(
            Store(calls.clone()),
            url::Url::parse("https://localhost:8443").unwrap(),
        ),
    );
    let body = json!({"revision":"0","backchannel_logout_uri":"https://client.example/logout","backchannel_logout_session_required":true});
    let response = app
        .clone()
        .oneshot(request().body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    for body in [
        json!({"revision":"0","backchannel_logout_uri":"http://client.example/logout","backchannel_logout_session_required":true}),
        json!({"revision":"0","backchannel_logout_uri":null,"backchannel_logout_session_required":false}),
        json!({"revision":"00","backchannel_logout_uri":null,"backchannel_logout_session_required":true}),
    ] {
        let response = app
            .clone()
            .oneshot(
                request()
                    .header("origin", "https://localhost:8443")
                    .header("x-darkhorse-csrf", "1")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let response = app
        .oneshot(
            request()
                .header("origin", "https://localhost:8443")
                .header("x-darkhorse-csrf", "1")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let value: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(value["revision"], "0");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
