//! Same-origin owner-only email verification. Raw proofs only arrive in bounded POST bodies.
use crate::{authentication_http, email_verification::token_digest, json::SafeJson};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use darkhorse_application::email_verification::{
    self as application, VerificationSecrets, VerificationStore,
};
use darkhorse_domain::email_verification::Error;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
struct Service<S, K> {
    store: S,
    secrets: K,
}
pub fn router<S: VerificationStore + 'static, K: VerificationSecrets + 'static>(
    store: S,
    secrets: K,
    origin: url::Url,
) -> Router {
    let service = Arc::new(Service { store, secrets });
    let routes = Router::new()
        .route("/api/security/email", get(status::<S, K>))
        .route("/api/security/email/request", post(request::<S, K>))
        .route("/api/security/email/confirm", post(confirm::<S, K>))
        .with_state(service);
    authentication_http::protect(routes, origin, 1024, 16).layer(
        tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ),
    )
}
async fn status<S: VerificationStore, K>(
    State(service): State<Arc<Service<S, K>>>,
    headers: HeaderMap,
) -> Response {
    let actor = match actor(&headers) {
        Ok(actor) => actor,
        Err(e) => return failure(e),
    };
    match service.store.email_status(actor).await {
        Ok(status) => {
            Json(json!({"email":status.email,"verified":status.verified})).into_response()
        }
        Err(e) => failure(e),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
async fn request<S: VerificationStore, K: VerificationSecrets>(
    State(service): State<Arc<Service<S, K>>>,
    headers: HeaderMap,
    SafeJson(_): SafeJson<Empty>,
) -> Response {
    let actor = match actor(&headers) {
        Ok(actor) => actor,
        Err(e) => return failure(e),
    };
    response(application::request(&service.store, &service.secrets, actor).await)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    token: String,
}
async fn confirm<S: VerificationStore, K>(
    State(service): State<Arc<Service<S, K>>>,
    headers: HeaderMap,
    SafeJson(input): SafeJson<Proof>,
) -> Response {
    let (actor, proof) = match confirmation(&headers, input) {
        Ok(values) => values,
        Err(e) => return failure(e),
    };
    response(service.store.verify_email(actor, proof).await)
}
fn confirmation(headers: &HeaderMap, input: Proof) -> Result<([u8; 32], [u8; 32]), Error> {
    let token = zeroize::Zeroizing::new(input.token);
    Ok((actor(headers)?, token_digest(&token)?))
}
fn actor(headers: &HeaderMap) -> Result<[u8; 32], Error> {
    authentication_http::cookie(headers)
        .map_err(|_| Error::Unauthorized)?
        .ok_or(Error::Unauthorized)
}
fn response(result: Result<(), Error>) -> Response {
    match result {
        Ok(()) => Json(json!({"ok":true})).into_response(),
        Err(e) => failure(e),
    }
}
fn failure(error: Error) -> Response {
    let (status, name) = match error {
        Error::Invalid => (StatusCode::BAD_REQUEST, "invalid_verification"),
        Error::Unauthorized => (StatusCode::UNAUTHORIZED, "sign_in_required"),
        Error::Throttled => (StatusCode::TOO_MANY_REQUESTS, "verification_limited"),
        Error::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable"),
    };
    let mut response = (status, Json(json!({"error":name}))).into_response();
    if error == Error::Unauthorized {
        authentication_http::clear_cookie(&mut response);
    }
    response
}
#[cfg(test)]
#[path = "../tests/unit/email_verification_http.rs"]
mod tests;
