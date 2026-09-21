use super::*;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::ServiceExt;
struct Fake {
    calls: Arc<AtomicUsize>,
    error: Option<Error>,
}
impl Store for Fake {
    async fn profile(&self, _: [u8; 32], _: Option<PrincipalId>) -> Result<Profile, Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.error.map_or_else(|| Ok(profile()), Err)
    }
    async fn update_profile(
        &self,
        actor: [u8; 32],
        target: Option<PrincipalId>,
        _: u64,
        _: Fields,
    ) -> Result<Profile, Error> {
        self.profile(actor, target).await
    }
}
fn profile() -> Profile {
    Profile {
        id: PrincipalId::from_u128(1).unwrap(),
        email: "ana@example.test".into(),
        active: true,
        email_verified: false,
        revision: 8,
        fields: prepare(serde_json::from_value(body()).unwrap()).unwrap().1,
    }
}
fn body() -> serde_json::Value {
    json!({"revision":"8","first_name":"Ana","second_name":"María","last_name":"Guzmán","second_last_name":"","country":"CR","calling_code":"","national_number":"","bio":"<script>plain text</script>"})
}
fn app(error: Option<Error>) -> (Router, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    (
        router(
            Fake {
                calls: calls.clone(),
                error,
            },
            url::Url::parse("https://identity.example").unwrap(),
        ),
        calls,
    )
}
fn request(path: &str, body: Option<serde_json::Value>) -> Request<Body> {
    Request::builder()
        .method(if body.is_some() { "POST" } else { "GET" })
        .uri(path)
        .header("host", "identity.example")
        .header("origin", "https://identity.example")
        .header("x-darkhorse-csrf", "1")
        .header("content-type", "application/json")
        .header("cookie", format!("__Host-darkhorse={}", "ab".repeat(32)))
        .body(body.map_or_else(Body::empty, |v| Body::from(v.to_string())))
        .unwrap()
}
#[tokio::test]
async fn profile_and_dropdown_responses_expose_only_the_documented_fields() {
    let (app, calls) = app(None);
    for input in [None, Some(body())] {
        let r = app
            .clone()
            .oneshot(request("/api/profiles/me", input))
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        assert_eq!(r.headers()["cache-control"], "no-store");
        let actual: serde_json::Value =
            serde_json::from_slice(&to_bytes(r.into_body(), 16384).await.unwrap()).unwrap();
        let mut expected = body();
        expected["id"] = uuid::Uuid::from_u128(1).to_string().into();
        expected["email"] = "ana@example.test".into();
        expected["active"] = true.into();
        expected["email_verified"] = false.into();
        assert_eq!(actual, expected);
    }
    let r = app
        .oneshot(request("/api/profiles/options", None))
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let actual: serde_json::Value =
        serde_json::from_slice(&to_bytes(r.into_body(), 65536).await.unwrap()).unwrap();
    assert_eq!(actual["countries"].as_array().unwrap().len(), 249);
    assert!(
        actual["calling_codes"]
            .as_array()
            .unwrap()
            .contains(&json!("506"))
    );
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}
#[tokio::test]
async fn malformed_unauthenticated_cross_origin_and_oversized_inputs_never_reach_the_store() {
    let (app, calls) = app(None);
    for (header, replacement, status) in [
        ("cookie", None, 401),
        ("origin", Some("https://other.example"), 403),
        ("x-darkhorse-csrf", None, 403),
        ("cookie", Some("__Host-darkhorse=invalid"), 401),
    ] {
        let mut r = request("/api/profiles/me", Some(body()));
        r.headers_mut().remove(header);
        if let Some(v) = replacement {
            r.headers_mut().insert(header, v.parse().unwrap());
        }
        assert_eq!(app.clone().oneshot(r).await.unwrap().status(), status);
    }
    for (path, input, status) in [
        ("/api/profiles/bad", body(), 400),
        ("/api/profiles/me", json!({"administrator":true}), 400),
        ("/api/profiles/me", json!({"bio":"x".repeat(16385)}), 413),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(request(path, Some(input)))
                .await
                .unwrap()
                .status(),
            status
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn store_failures_use_fixed_errors_and_only_invalid_sessions_clear_the_cookie() {
    for (error, status) in [
        (Error::Invalid, 400),
        (Error::Unauthorized, 401),
        (Error::Forbidden, 403),
        (Error::RecentAuthentication, 403),
        (Error::NotFound, 404),
        (Error::Conflict, 409),
        (Error::Unavailable, 503),
    ] {
        for (path, input) in [
            ("/api/profiles/me", None),
            ("/api/profiles/me", Some(body())),
            ("/api/profiles/options", None),
        ] {
            let (app, _) = app(Some(error));
            let r = app.oneshot(request(path, input)).await.unwrap();
            assert_eq!(r.status(), status);
            assert_eq!(r.headers().contains_key("set-cookie"), status == 401);
            assert_eq!(r.headers()["cache-control"], "no-store");
            let body: serde_json::Value =
                serde_json::from_slice(&to_bytes(r.into_body(), 1024).await.unwrap()).unwrap();
            assert_eq!(body.as_object().unwrap().len(), 1);
            assert!(body["error"].is_string());
        }
    }
}
