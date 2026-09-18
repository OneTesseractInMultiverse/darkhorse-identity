use super::{Fixture, SERIAL};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use darkhorse_adapters::{
    authentication_http, http, login_admission::SharedLoginAdmission,
    password::PasswordPreparation, session_secret::OsSessionEntropy,
};
use darkhorse_application::{
    authentication::Service,
    bootstrap::{BootstrapRequest, bootstrap},
};
use tower::ServiceExt;

fn request(email: &str, password: &str) -> Request<Body> {
    Request::builder()
        .uri("/api/auth/login")
        .method("POST")
        .header("host", "localhost:8443")
        .header("origin", "https://localhost:8443")
        .header("x-darkhorse-csrf", "1")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"email":email,"password":password}).to_string(),
        ))
        .unwrap()
}
fn router(f: &Fixture) -> Router {
    http::with_authentication(
        "unused-integration-assets".into(),
        authentication_http::router(
            Service {
                store: f.store.clone(),
                admission: SharedLoginAdmission::new(f.limiter(), [7; 32]),
                passwords: PasswordPreparation::default(),
                entropy: OsSessionEntropy,
            },
            url::Url::parse("https://localhost:8443").unwrap(),
        ),
    )
}
#[tokio::test]
async fn real_password_http_sessions_shared_budgets_and_fenced_limiter() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    let password = "test-only login passphrase";
    bootstrap(
        &f.store,
        &PasswordPreparation::default(),
        BootstrapRequest {
            email: "login@example.com",
            first_name: "Login",
            last_name: "Test",
            password,
        },
    )
    .await
    .unwrap();
    let app = router(&f);
    let response = app
        .clone()
        .oneshot(request(" LOGIN@example.com ", password))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let view = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/auth/session")
                .header("host", "localhost:8443")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(view.status(), StatusCode::OK);
    let wrong = app
        .clone()
        .oneshot(request("login@example.com", "wrong"))
        .await
        .unwrap();
    let unknown = app
        .clone()
        .oneshot(request("unknown@example.com", "wrong"))
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(unknown.status(), wrong.status());
    assert_eq!(
        to_bytes(wrong.into_body(), 1024).await.unwrap(),
        to_bytes(unknown.into_body(), 1024).await.unwrap()
    );
    for _ in 0..3 {
        assert_eq!(
            app.clone()
                .oneshot(request("login@example.com", "wrong"))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    let second_replica = router(&f);
    assert_eq!(
        second_replica
            .oneshot(request("LOGIN@example.com", password))
            .await
            .unwrap()
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    f.fence().await;
    assert_eq!(
        app.oneshot(request("different@example.com", password))
            .await
            .unwrap()
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}
