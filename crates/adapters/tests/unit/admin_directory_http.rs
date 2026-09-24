use super::*;
#[test]
fn projections_exclude_internal_authority_and_preserve_large_revisions() {
    let body = user(User {
        id: PrincipalId::from_u128(1).unwrap(),
        email: "one@example.com".into(),
        first_name: "Ada".into(),
        last_name: "Lovelace".into(),
        status: AccountStatus::Active,
        administrator: true,
        email_verified: false,
        revision: i64::MAX as u64,
    });
    assert_eq!(body["revision"], "9223372036854775807");
    assert_eq!(body.as_object().unwrap().len(), 8);
    assert_eq!(counter("9223372036854775807"), Ok(i64::MAX as u64));
    for input in ["-1", "01", "1.5", "9223372036854775808"] {
        assert_eq!(counter(input), Err(Error::Invalid));
    }
    assert!(
        selected(Some("application=00000000-0000-0000-0000-000000000001"))
            .unwrap()
            .is_some()
    );
    for input in [
        "application=bad",
        "application=00000000-0000-0000-0000-000000000001&application=bad",
        "other=x",
    ] {
        assert_eq!(selected(Some(input)), Err(Error::Invalid));
    }
    assert!(selected(None).unwrap().is_none());
    assert!(matches!(
        change(InputChange::Names {
            first_name: "Ada".into(),
            last_name: "Lovelace".into()
        }),
        Ok(Change::Names(_))
    ));
}
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tower::ServiceExt;
struct Fake {
    calls: Arc<AtomicUsize>,
    changes: Arc<Mutex<Vec<Change>>>,
    error: Option<Error>,
}
fn record() -> User {
    User {
        id: PrincipalId::from_u128(1).unwrap(),
        email: "one@example.com".into(),
        first_name: "Ada".into(),
        last_name: "Lovelace".into(),
        status: AccountStatus::Active,
        administrator: true,
        email_verified: false,
        revision: 7,
    }
}
impl Fake {
    fn result(&self, actor: [u8; 32]) -> Result<User, Error> {
        assert_eq!(
            actor,
            crate::session_secret::digest(&"a".repeat(64)).unwrap()
        );
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.error.map_or_else(|| Ok(record()), Err)
    }
}
impl AdminDirectory for Fake {
    async fn users(
        &self,
        actor: [u8; 32],
        query: darkhorse_domain::admin_directory::Query,
    ) -> Result<Page, Error> {
        let user = self.result(actor)?;
        query.validate()?;
        Ok(Page {
            actor: user.id,
            items: vec![user],
            next: None,
        })
    }
    async fn user(&self, actor: [u8; 32], id: PrincipalId) -> Result<User, Error> {
        assert_eq!(id.as_u128(), 1);
        self.result(actor)
    }
    async fn access(
        &self,
        actor: [u8; 32],
        id: PrincipalId,
        application: Option<ApplicationId>,
    ) -> Result<Access, Error> {
        assert_eq!(id.as_u128(), 1);
        let user = self.result(actor)?;
        let app = application.unwrap_or(ApplicationId::from_u128(2).unwrap());
        Ok(Access {
            user,
            policy_revision: 8,
            applications: vec![ApplicationSummary {
                id: app,
                name: "Portal".into(),
                active: true,
            }],
            selected: Some(app),
            roles: vec![Role {
                id: RoleId::from_u128(3).unwrap(),
                name: "Reader".into(),
                assigned: false,
            }],
        })
    }
    async fn update(
        &self,
        actor: [u8; 32],
        id: PrincipalId,
        revision: u64,
        change: Change,
    ) -> Result<User, Error> {
        assert_eq!((id.as_u128(), revision), (1, 7));
        self.changes.lock().unwrap().push(change);
        self.result(actor)
    }
}
fn app(error: Option<Error>) -> (Router, Arc<AtomicUsize>, Arc<Mutex<Vec<Change>>>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let changes = Arc::new(Mutex::new(vec![]));
    let router = router(
        Fake {
            calls: calls.clone(),
            changes: changes.clone(),
            error,
        },
        url::Url::parse("https://localhost:8443").unwrap(),
    );
    (
        crate::http::with_authentication(std::path::PathBuf::new(), router),
        calls,
        changes,
    )
}
fn request(method: &str, path: &str) -> axum::http::request::Builder {
    Request::builder()
        .method(method)
        .uri(path)
        .header("host", "localhost:8443")
        .header("origin", "https://localhost:8443")
        .header("x-darkhorse-csrf", "1")
        .header("content-type", "application/json")
        .header("cookie", format!("__Host-darkhorse={}", "a".repeat(64)))
}
fn target() -> String {
    format!("/api/admin/users/{}", id(1))
}
#[tokio::test]
async fn directory_http_translates_exact_commands_and_projects_safe_records() {
    let (app, calls, changes) = app(None);
    for path in [
        "/api/admin/users?limit=2&search=%25_".into(),
        target(),
        format!("{}/access?application={}", target(), id(2)),
    ] {
        let response = app
            .clone()
            .oneshot(request("GET", &path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body = to_bytes(response.into_body(), 8192).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(!text.contains("credential"));
        assert!(!text.contains("epoch"));
        assert!(!text.contains("digest"));
    }
    for change in [
        json!({"kind":"names","first_name":"Grace","last_name":"Hopper"}),
        json!({"kind":"status","active":false}),
        json!({"kind":"status","active":true}),
        json!({"kind":"role","application_id":id(2),"role_id":id(3),"assigned":true,"policy_revision":"8"}),
    ] {
        let response = app
            .clone()
            .oneshot(
                request("POST", &target())
                    .body(Body::from(
                        json!({"revision":"7","change":change}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 7);
    let changes = changes.lock().unwrap();
    assert!(matches!(&changes[0],Change::Names(n) if n.first=="Grace"));
    assert!(matches!(
        changes[1],
        Change::Status(AccountStatus::Inactive)
    ));
    assert!(
        matches!(changes[3],Change::Role{application,role,policy_revision:8,assigned:true} if application.as_u128()==2 && role.as_u128()==3)
    );
}
#[tokio::test]
async fn directory_http_rejects_ambiguous_or_untrusted_inputs_before_the_port() {
    let (app, calls, _) = app(None);
    for path in [
        "/api/admin/users?skip=2".into(),
        "/api/admin/users?limit=101".into(),
        "/api/admin/users?search=x&search=y".into(),
        format!("{}?email=x", target()),
        format!("{}/access?application=bad", target()),
        "/api/admin/users/not-a-uuid".into(),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(request("GET", &path).body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let valid = json!({"revision":"7","change":{"kind":"status","active":false}});
    for body in [
        json!({"revision":7,"change":{"kind":"status","active":false}}),
        json!({"revision":"07","change":{"kind":"status","active":false}}),
        json!({"revision":"7","change":{"kind":"names","first_name":"","last_name":"L"}}),
        json!({"revision":"7","change":{"kind":"status","active":false,"administrator":true}}),
        json!({"revision":"7","change":{"kind":"role","application_id":id(0),"role_id":id(3),"assigned":true,"policy_revision":"8"}}),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(
                    request("POST", &target())
                        .body(Body::from(body.to_string()))
                        .unwrap()
                )
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    for (header, value) in [
        ("origin", "https://evil.example"),
        ("x-darkhorse-csrf", "0"),
        ("sec-fetch-site", "cross-site"),
    ] {
        let mut req = request("POST", &target())
            .body(Body::from(valid.to_string()))
            .unwrap();
        req.headers_mut().insert(header, value.parse().unwrap());
        assert_eq!(
            app.clone().oneshot(req).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }
    let mut req = request("GET", "/api/admin/users")
        .body(Body::empty())
        .unwrap();
    req.headers_mut().remove("cookie");
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(response.headers().contains_key("set-cookie"));
    assert_eq!(
        app.clone()
            .oneshot(
                request("POST", &format!("{}?ignored=1", target()))
                    .body(Body::from(valid.to_string()))
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.oneshot(
            request("POST", &target())
                .body(Body::from("x".repeat(4097)))
                .unwrap()
        )
        .await
        .unwrap()
        .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn directory_http_failure_status_is_consistent_for_all_endpoints() {
    for (error, status) in [
        (Error::Invalid, 400),
        (Error::Unauthorized, 401),
        (Error::Forbidden, 403),
        (Error::RecentAuthentication, 403),
        (Error::NotFound, 404),
        (Error::Conflict, 409),
        (Error::LastAdministrator, 409),
        (Error::Unavailable, 503),
    ] {
        let (app, _, _) = app(Some(error));
        for (method, path, body) in [
            ("GET", "/api/admin/users".into(), String::new()),
            ("GET", target(), String::new()),
            ("GET", format!("{}/access", target()), String::new()),
            (
                "POST",
                target(),
                json!({"revision":"7","change":{"kind":"status","active":false}}).to_string(),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(request(method, &path).body(Body::from(body)).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), status);
            assert_eq!(response.headers()["cache-control"], "no-store");
            let bytes = to_bytes(response.into_body(), 1024).await.unwrap();
            let body: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(body.as_object().unwrap().len(), 1);
        }
    }
}

#[tokio::test]
async fn query_value_limit_rejections_are_redacted_before_service_calls() {
    let (app, calls, _) = app(None);
    let path = format!("/api/admin/users?limit={}25", "%30".repeat(30));
    let response = app
        .clone()
        .oneshot(request("GET", &path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    for query in [
        format!("limit={}25", "%30".repeat(31)),
        "status=private%3E%3Dvalue".into(),
        "private%0Afield=value".into(),
        "status=%00private".into(),
        "status=/private/".into(),
    ] {
        let path = format!("/api/admin/users?{query}");
        let response = app
            .clone()
            .oneshot(request("GET", &path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            json!({"error":"invalid_request"})
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
