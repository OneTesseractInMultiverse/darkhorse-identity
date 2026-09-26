use super::*;
use axum::{
    body::{Body, to_bytes},
    http::Request as HttpRequest,
};
use darkhorse_application::tokens::Opaque;
use darkhorse_domain::{
    localization::Locale,
    oidc::{Interaction, Request},
};
use std::{collections::BTreeMap, sync::Mutex};
use tower::ServiceExt;
#[derive(Clone, Default)]
struct Store(Arc<Mutex<BTreeMap<[u8; 32], Option<Locale>>>>);
impl AuthorizationStore for Store {
    async fn begin(
        &self,
        request: Request,
        handle: [u8; 32],
        _: Option<[u8; 32]>,
    ) -> Result<Outcome, Error> {
        self.0.lock().unwrap().insert(handle, request.ui_locale);
        Ok(pending(request.ui_locale))
    }
    async fn resume(
        &self,
        handle: [u8; 32],
        _: Option<[u8; 32]>,
        decision: Decision,
    ) -> Result<Outcome, Error> {
        let locale = *self
            .0
            .lock()
            .unwrap()
            .get(&handle)
            .ok_or(Error::InvalidTransaction)?;
        Ok(match decision {
            Decision::Inspect => pending(locale),
            Decision::Deny => Outcome::Return {
                target: ReturnTo {
                    uri: "https://client.example/callback".into(),
                    state: None,
                },
                error: Error::AccessDenied,
            },
            Decision::Approve => Outcome::Pending(View {
                client_name: "Test".into(),
                scopes: vec!["openid".into()],
                resource: None,
                interaction: Interaction::Ready,
                ui_locale: locale,
            }),
        })
    }
}
impl CodeStore for Store {
    async fn issue(
        &self,
        _: [u8; 32],
        _: Option<[u8; 32]>,
        code: Opaque,
    ) -> Result<Code, darkhorse_domain::tokens::Error> {
        Ok(Code {
            target: ReturnTo {
                uri: "https://client.example/callback".into(),
                state: None,
            },
            value: code.value,
        })
    }
}
fn pending(ui_locale: Option<Locale>) -> Outcome {
    Outcome::Pending(View {
        client_name: "Test".into(),
        scopes: vec!["openid".into()],
        resource: None,
        interaction: Interaction::Consent,
        ui_locale,
    })
}
fn app() -> Router {
    let origin = url::Url::parse("https://identity.example").unwrap();
    let state = Arc::new(Provider {
        store: Store::default(),
        issuer: origin.origin().ascii_serialization(),
    });
    authentication_http::protect_with_queries(
        Router::new()
            .route("/authorize", get(begin::<Store>))
            .route("/api/authorization", get(inspect::<Store>))
            .route("/api/authorization/decision", post(decide::<Store>))
            .with_state(state),
        origin,
        1024,
        32,
        true,
    )
}
async fn call(app: &Router, uri: &str, cookie: &str, body: Option<serde_json::Value>) -> Response {
    let mut request = HttpRequest::builder()
        .uri(uri)
        .header("host", "identity.example");
    if !cookie.is_empty() {
        request = request.header(header::COOKIE, cookie);
    }
    if body.is_some() {
        request = request
            .method("POST")
            .header("origin", "https://identity.example")
            .header("x-darkhorse-csrf", "1")
            .header("content-type", "application/json");
    }
    app.clone()
        .oneshot(
            request
                .body(Body::from(body.map(|v| v.to_string()).unwrap_or_default()))
                .unwrap(),
        )
        .await
        .unwrap()
}
async fn begin_flow(app: &Router, locale: &str) -> (String, String) {
    let query = format!(
        "/authorize?client_id=00000000-0000-0000-0000-000000000020&redirect_uri=https%3A%2F%2Fclient.example%2Fcallback&response_type=code&scope=openid&code_challenge={}&code_challenge_method=S256&ui_locales={locale}",
        "A".repeat(43)
    );
    let response = call(app, &query, "", None).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let reference = response.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .strip_prefix("/authorization?request=")
        .unwrap()
        .to_owned();
    let cookie = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    (reference, cookie)
}
#[tokio::test]
async fn two_flows_retain_language_and_only_the_selected_proof_is_expired() {
    let app = app();
    let (es, first) = begin_flow(&app, "es-CR").await;
    let (en, second) = begin_flow(&app, "en").await;
    let cookies = format!("{first}; {second}");
    for (id, locale) in [(&es, "es"), (&en, "en"), (&es, "es")] {
        let response = call(
            &app,
            &format!("/api/authorization?request={id}"),
            &cookies,
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let value: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(value["request_id"], *id);
        assert_eq!(value["ui_locale"], locale);
    }
    for (id, choice) in [(&es, "deny"), (&en, "approve")] {
        let response = call(
            &app,
            &format!("/api/authorization/decision?request={id}"),
            &cookies,
            Some(serde_json::json!({"request_id":id,"decision":choice})),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::SET_COOKIE],
            context::cookie(id, "", 0).unwrap()
        );
    }
}
#[tokio::test]
async fn missing_ambiguous_or_mismatched_selectors_never_resume_a_different_flow() {
    let app = app();
    let (es, first) = begin_flow(&app, "es").await;
    let (en, second) = begin_flow(&app, "en").await;
    let cookies = format!("{first}; {second}");
    for uri in [
        "/api/authorization".into(),
        format!("/api/authorization?request={es}&request={en}"),
    ] {
        assert_eq!(
            call(&app, &uri, &cookies, None).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
    let response = call(
        &app,
        &format!("/api/authorization/decision?request={en}"),
        &cookies,
        Some(serde_json::json!({"request_id":es,"decision":"approve"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        call(
            &app,
            &format!("/api/authorization?request={es}"),
            &second,
            None
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn head_cannot_resume_or_complete_an_authorization_flow() {
    let app = app();
    let (reference, cookie) = begin_flow(&app, "es").await;
    let response = app
        .oneshot(
            HttpRequest::builder()
                .method("HEAD")
                .uri(format!("/api/authorization?request={reference}"))
                .header("host", "identity.example")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}
