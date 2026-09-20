use super::*;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use darkhorse_application::personal_keys::*;
use darkhorse_domain::personal_keys::{ExpiryPolicy, Request as KeyRequest};
use tower::ServiceExt;
#[derive(Clone)]
struct Fake {
    error: Option<Error>,
}
impl Entropy for Fake {
    fn key(&self) -> Result<Prepared, Error> {
        Ok(Prepared {
            value: format!("dk_{}", "ab".repeat(32)),
            verifier: Verifier {
                id: CredentialId::from_u128(3).unwrap(),
                digest: [3; 32],
            },
        })
    }
}
fn record() -> Record {
    Record {
        id: CredentialId::from_u128(3).unwrap(),
        name: "worker".into(),
        application: ApplicationId::from_u128(1).unwrap(),
        created_ms: 1000,
        expires_ms: None,
        active: true,
        grants: vec![Grant {
            resource: ResourceId::from_u128(2).unwrap(),
            ceiling: [CapabilityId::from_u128(4).unwrap()].into(),
        }],
    }
}
impl Store for Fake {
    async fn options(&self, _: [u8; 32], _: Option<ResourceId>) -> Result<Options, Error> {
        if let Some(e) = self.error {
            return Err(e);
        }
        Ok(Options {
            policy: ExpiryPolicy::new(30, 365, true).unwrap(),
            revision: 9,
            items: vec![Eligible {
                application: ApplicationId::from_u128(1).unwrap(),
                application_name: "App".into(),
                resource: ResourceId::from_u128(2).unwrap(),
                resource_name: "API".into(),
                capabilities: vec![Capability {
                    id: CapabilityId::from_u128(4).unwrap(),
                    key: "read".into(),
                    meaning: "Read records".into(),
                }],
            }],
            next: Some(ResourceId::from_u128(2).unwrap()),
        })
    }
    async fn list(&self, _: [u8; 32], _: Option<CredentialId>) -> Result<Page, Error> {
        if let Some(e) = self.error {
            return Err(e);
        }
        Ok(Page {
            items: vec![record()],
            next: Some(CredentialId::from_u128(3).unwrap()),
        })
    }
    async fn preflight(&self, _: [u8; 32], _: &KeyRequest) -> Result<(), Error> {
        self.error.map_or(Ok(()), Err)
    }
    async fn issue(&self, _: [u8; 32], _: &KeyRequest, _: Verifier) -> Result<Record, Error> {
        Ok(record())
    }
    async fn revoke(&self, _: [u8; 32], _: CredentialId) -> Result<(), Error> {
        self.error.map_or(Ok(()), Err)
    }
}
fn app(error: Option<Error>) -> Router {
    router(
        Service {
            store: Fake { error },
            entropy: Fake { error },
        },
        url::Url::parse("https://identity.example").unwrap(),
    )
}
fn request(path: &str, body: Option<serde_json::Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .uri(path)
        .header("host", "identity.example")
        .header("cookie", format!("__Host-darkhorse={}", "ab".repeat(32)));
    if body.is_some() {
        builder = builder
            .method("POST")
            .header("origin", "https://identity.example")
            .header("x-darkhorse-csrf", "1")
            .header("content-type", "application/json");
    }
    builder
        .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
        .unwrap()
}
fn creation() -> serde_json::Value {
    serde_json::json!({"name":"worker","application_id":uuid::Uuid::from_u128(1).to_string(),"policy_revision":"9","expiration":{"kind":"default"},"grants":[{"resource_id":uuid::Uuid::from_u128(2).to_string(),"selection":{"kind":"all"}}]})
}
#[tokio::test]
async fn protected_metadata_and_single_reveal_never_echo_untrusted_failures() {
    for path in ["/api/security/keys", "/api/security/keys/options"] {
        let response = app(None).oneshot(request(path, None)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body = to_bytes(response.into_body(), 16384).await.unwrap();
        assert!(!String::from_utf8_lossy(&body).contains("secret"));
    }
    let response = app(None)
        .oneshot(request("/api/security/keys", Some(creation())))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 16384).await.unwrap()).unwrap();
    assert!(body["secret"].as_str().unwrap().starts_with("dk_"));
    let revoke = serde_json::json!({"key_id":uuid::Uuid::from_u128(3).to_string()});
    assert_eq!(
        app(None)
            .oneshot(request("/api/security/keys/revoke", Some(revoke.clone())))
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    for (e, status) in [
        (Error::Unauthorized, 401),
        (Error::Forbidden, 403),
        (Error::RecentAuthenticationRequired, 403),
        (Error::NotFound, 404),
        (Error::Conflict, 409),
        (Error::Limit, 429),
        (Error::Unavailable, 503),
        (Error::Invalid, 400),
    ] {
        for (path, body) in [
            ("/api/security/keys", Some(creation())),
            ("/api/security/keys", None),
            ("/api/security/keys/options", None),
            ("/api/security/keys/revoke", Some(revoke.clone())),
        ] {
            let response = app(Some(e)).oneshot(request(path, body)).await.unwrap();
            assert_eq!(response.status().as_u16(), status);
        }
    }
    let mut missing = request("/api/security/keys", None);
    missing.headers_mut().remove("cookie");
    assert_eq!(app(None).oneshot(missing).await.unwrap().status(), 401);
    for (name, value) in [
        ("origin", "https://evil.example"),
        ("x-darkhorse-csrf", "0"),
    ] {
        let mut denied = request("/api/security/keys", Some(creation()));
        denied.headers_mut().insert(name, value.parse().unwrap());
        assert_eq!(app(None).oneshot(denied).await.unwrap().status(), 403);
    }
    assert_eq!(
        app(None)
            .oneshot(request("/api/security/keys?after=bad", None))
            .await
            .unwrap()
            .status(),
        400
    );
    assert_eq!(
        app(None)
            .oneshot(request("/api/security/keys/options?after=bad", None))
            .await
            .unwrap()
            .status(),
        400
    );
    assert_eq!(
        app(None)
            .oneshot(request(
                "/api/security/keys/revoke",
                Some(serde_json::json!({"key_id":"bad"}))
            ))
            .await
            .unwrap()
            .status(),
        400
    );
    let mut invalid = creation();
    invalid["policy_revision"] = "bad".into();
    assert_eq!(
        app(None)
            .oneshot(request("/api/security/keys", Some(invalid)))
            .await
            .unwrap()
            .status(),
        400
    );
}
