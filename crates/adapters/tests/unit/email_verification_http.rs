use super::*;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use darkhorse_application::email_verification::{Material, Status};
use darkhorse_domain::identity::EmailVerificationId;
use tower::ServiceExt;
struct Fake(Option<Error>);
impl VerificationStore for Fake {
    async fn email_status(&self, _: [u8; 32]) -> Result<Status, Error> {
        self.0.map_or(
            Ok(Status {
                email: "one@example.com".into(),
                verified: true,
            }),
            Err,
        )
    }
    async fn request_verification(&self, _: [u8; 32], _: Material) -> Result<(), Error> {
        self.0.map_or(Ok(()), Err)
    }
    async fn verify_email(&self, _: [u8; 32], _: [u8; 32]) -> Result<(), Error> {
        self.0.map_or(Ok(()), Err)
    }
}
struct Secrets;
impl VerificationSecrets for Secrets {
    fn issue(&self) -> Result<Material, Error> {
        Ok(Material {
            id: EmailVerificationId::from_u128(1).unwrap(),
            seed: [2; 32],
            digest: [3; 32],
        })
    }
}
fn app(error: Option<Error>) -> Router {
    router(
        Fake(error),
        Secrets,
        url::Url::parse("https://localhost:8443").unwrap(),
    )
}
fn req(method: &str, path: &str, body: String) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("host", "localhost:8443")
        .header("origin", "https://localhost:8443")
        .header("x-darkhorse-csrf", "1")
        .header("content-type", "application/json")
        .header("cookie", format!("__Host-darkhorse={}", "a".repeat(64)))
        .body(Body::from(body))
        .unwrap()
}
#[tokio::test]
async fn bounded_transport_requires_live_actor_same_origin_and_body_proof() {
    let response = app(None)
        .oneshot(req("GET", "/api/security/email", String::new()))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
        json!({"email":"one@example.com","verified":true})
    );
    for path in ["/api/security/email/request", "/api/security/email/confirm"] {
        let body = if path.ends_with("confirm") {
            format!("{{\"token\":\"ev1_{}\"}}", "a".repeat(64))
        } else {
            "{}".into()
        };
        assert_eq!(
            app(None)
                .oneshot(req("POST", path, body.clone()))
                .await
                .unwrap()
                .status(),
            200
        );
        for (error, status) in [
            (Error::Unauthorized, 401),
            (Error::Invalid, 400),
            (Error::Throttled, 429),
            (Error::Unavailable, 503),
        ] {
            let response = app(Some(error))
                .oneshot(req("POST", path, body.clone()))
                .await
                .unwrap();
            assert_eq!(response.status(), status);
            assert_eq!(
                response.headers().contains_key("set-cookie"),
                error == Error::Unauthorized
            );
        }
        for removed in ["origin", "x-darkhorse-csrf", "cookie"] {
            let mut request = req("POST", path, body.clone());
            request.headers_mut().remove(removed);
            assert!(
                !app(None)
                    .oneshot(request)
                    .await
                    .unwrap()
                    .status()
                    .is_success()
            );
        }
        assert_eq!(
            app(None)
                .oneshot(req("POST", path, "{\"extra\":true}".into()))
                .await
                .unwrap()
                .status(),
            400
        );
        assert_eq!(
            app(None)
                .oneshot(req("POST", path, "x".repeat(2048)))
                .await
                .unwrap()
                .status(),
            413
        );
    }
    assert_eq!(
        app(None)
            .oneshot(req(
                "POST",
                "/api/security/email/confirm",
                "{\"token\":\"bad\"}".into()
            ))
            .await
            .unwrap()
            .status(),
        400
    );
    let mut no_actor = req("GET", "/api/security/email", String::new());
    no_actor.headers_mut().remove("cookie");
    assert_eq!(app(None).oneshot(no_actor).await.unwrap().status(), 401);
    assert_eq!(
        app(Some(Error::Unavailable))
            .oneshot(req("GET", "/api/security/email", String::new()))
            .await
            .unwrap()
            .status(),
        503
    );
}
