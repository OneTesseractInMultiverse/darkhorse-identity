use super::*;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use darkhorse_application::credentials::PreparedCredential;
use darkhorse_domain::{
    directory::Profile,
    identity::{CredentialId, PrincipalId},
};
use tower::ServiceExt;
struct Fake(Option<Error>);
fn id() -> InvitationId {
    InvitationId::from_u128(3).unwrap()
}
impl InvitationSecrets for Fake {
    fn issue_invitation(&self) -> Result<Material, Error> {
        Ok(Material {
            id: id(),
            seed: [2; 32],
            digest: [3; 32],
        })
    }
}
impl PasswordPreparation for Fake {
    async fn prepare_password(&self, _: &str) -> Result<PreparedCredential, Error> {
        Ok(PreparedCredential {
            principal_id: PrincipalId::from_u128(1).unwrap(),
            credential_id: CredentialId::from_u128(2).unwrap(),
            verifier: "prepared".into(),
        })
    }
}
impl InvitationStore for Fake {
    async fn invitation_preflight(&self, _: [u8; 32]) -> Result<(), Error> {
        self.0.map_or(Ok(()), Err)
    }
    async fn invite(&self, _: [u8; 32], _: &str, _: Material) -> Result<InvitationId, Error> {
        self.0.map_or(Ok(id()), Err)
    }
    async fn invitations(&self, _: [u8; 32]) -> Result<Vec<Record>, Error> {
        self.0.map_or(
            Ok(vec![Record {
                id: id(),
                email: "new@example.com".into(),
                created_ms: 100,
                expires_ms: 200,
                closed: false,
                delivery: "queued".into(),
            }]),
            Err,
        )
    }
    async fn revoke_invitation(&self, _: [u8; 32], _: InvitationId) -> Result<(), Error> {
        self.0.map_or(Ok(()), Err)
    }
    async fn admit_invitation(&self, _: [u8; 32], _: &str) -> Result<InvitationId, Error> {
        self.0.map_or(Ok(id()), Err)
    }
    async fn accept_invitation(
        &self,
        _: InvitationId,
        _: [u8; 32],
        _: Profile,
        c: PreparedCredential,
    ) -> Result<PrincipalId, Error> {
        self.0.map_or(Ok(c.principal_id), Err)
    }
}
fn app(error: Option<Error>) -> Router {
    router(
        Fake(error),
        Fake(None),
        Fake(None),
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
fn acceptance() -> String {
    json!({"token":format!("iv1_{}","a".repeat(64)),"email":"new@example.com","first_name":"New","last_name":"Person","password":"a long test password"}).to_string()
}
#[tokio::test]
async fn admin_transport_has_explicit_errors_no_proofs_and_live_actor_requirement() {
    let response = app(None)
        .oneshot(req("GET", "/api/admin/invitations", String::new()))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(body["invitations"][0]["email"], "new@example.com");
    assert!(body["invitations"][0].get("seed").is_none());
    for (path, body, success) in [
        (
            "/api/admin/invitations",
            "{\"email\":\"new@example.com\"}".to_owned(),
            201,
        ),
        (
            "/api/admin/invitations/revoke",
            json!({"id":uuid::Uuid::from_u128(3).to_string()}).to_string(),
            200,
        ),
        ("/api/invitations/accept", acceptance(), 200),
    ] {
        assert_eq!(
            app(None)
                .oneshot(req("POST", path, body.clone()))
                .await
                .unwrap()
                .status(),
            success
        );
        for (error, status) in [
            (Error::Invalid, 400),
            (Error::Unauthorized, 401),
            (Error::Forbidden, 403),
            (Error::RecentAuthentication, 403),
            (Error::Conflict, 409),
            (Error::Throttled, 429),
            (Error::Unavailable, 503),
        ] {
            assert_eq!(
                app(Some(error))
                    .oneshot(req("POST", path, body.clone()))
                    .await
                    .unwrap()
                    .status(),
                status
            );
        }
        for removed in ["origin", "x-darkhorse-csrf"] {
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
                .oneshot(req("POST", path, "x".repeat(4097)))
                .await
                .unwrap()
                .status(),
            413
        );
        assert_eq!(
            app(None)
                .oneshot(req("POST", path, "{\"extra\":true}".into()))
                .await
                .unwrap()
                .status(),
            400
        );
    }
    let mut request = req("POST", "/api/invitations/accept", acceptance());
    request.headers_mut().remove("cookie");
    assert_eq!(app(None).oneshot(request).await.unwrap().status(), 200);
    for method in ["GET", "POST"] {
        let mut request = req(
            method,
            "/api/admin/invitations",
            "{\"email\":\"new@example.com\"}".into(),
        );
        request.headers_mut().remove("cookie");
        assert_eq!(app(None).oneshot(request).await.unwrap().status(), 401);
    }
    let mut request = req(
        "POST",
        "/api/admin/invitations/revoke",
        json!({"id":uuid::Uuid::from_u128(3).to_string()}).to_string(),
    );
    request.headers_mut().remove("cookie");
    assert_eq!(app(None).oneshot(request).await.unwrap().status(), 401);
    assert_eq!(
        app(Some(Error::Unavailable))
            .oneshot(req("GET", "/api/admin/invitations", String::new()))
            .await
            .unwrap()
            .status(),
        503
    );
    for id in ["bad", "00000000-0000-0000-0000-000000000000"] {
        assert_eq!(
            app(None)
                .oneshot(req(
                    "POST",
                    "/api/admin/invitations/revoke",
                    json!({"id":id}).to_string()
                ))
                .await
                .unwrap()
                .status(),
            400
        );
    }
    assert_eq!(
        app(None)
            .oneshot(req(
                "POST",
                "/api/invitations/accept",
                acceptance().replace("iv1_", "ev1_")
            ))
            .await
            .unwrap()
            .status(),
        400
    );
}
