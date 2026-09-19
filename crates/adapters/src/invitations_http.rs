//! Bounded same-origin invitation administration and public proof redemption.
use crate::{authentication_http, email_verification::invitation_digest, json::SafeJson};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use darkhorse_application::invitations::{self as application, *};
use darkhorse_domain::{identity::InvitationId, invitations::Error};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
struct Service<S, K, P> {
    store: S,
    secrets: K,
    passwords: P,
}
pub fn router<
    S: InvitationStore + 'static,
    K: InvitationSecrets + 'static,
    P: PasswordPreparation + 'static,
>(
    store: S,
    secrets: K,
    passwords: P,
    origin: url::Url,
) -> Router {
    let routes = Router::new()
        .route(
            "/api/admin/invitations",
            get(list::<S, K, P>).post(invite::<S, K, P>),
        )
        .route("/api/admin/invitations/revoke", post(revoke::<S, K, P>))
        .route("/api/invitations/accept", post(accept::<S, K, P>))
        .with_state(Arc::new(Service {
            store,
            secrets,
            passwords,
        }));
    authentication_http::protect(routes, origin, 4096, 16).layer(
        tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ),
    )
}
async fn list<S: InvitationStore, K, P>(
    State(service): State<Arc<Service<S, K, P>>>,
    headers: HeaderMap,
) -> Response {
    let actor = match actor(&headers) {
        Ok(actor) => actor,
        Err(e) => return failure(e),
    };
    match service.store.invitations(actor).await {
        Ok(records) => {
            Json(json!({"invitations":records.iter().map(record).collect::<Vec<_>>(),"limit":100}))
                .into_response()
        }
        Err(e) => failure(e),
    }
}
fn record(r: &Record) -> serde_json::Value {
    json!({"id":uuid::Uuid::from_u128(r.id.as_u128()).to_string(),"email":r.email,"created_ms":r.created_ms,"expires_ms":r.expires_ms,"closed":r.closed,"delivery":r.delivery})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Invite {
    email: String,
}
async fn invite<S: InvitationStore, K: InvitationSecrets, P>(
    State(service): State<Arc<Service<S, K, P>>>,
    headers: HeaderMap,
    SafeJson(input): SafeJson<Invite>,
) -> Response {
    let actor = match actor(&headers) {
        Ok(actor) => actor,
        Err(e) => return failure(e),
    };
    match application::invite(&service.store, &service.secrets, actor, &input.email).await {
        Ok(id) => (
            StatusCode::CREATED,
            Json(json!({"id":uuid::Uuid::from_u128(id.as_u128()).to_string()})),
        )
            .into_response(),
        Err(e) => failure(e),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revoke {
    id: String,
}
async fn revoke<S: InvitationStore, K, P>(
    State(service): State<Arc<Service<S, K, P>>>,
    headers: HeaderMap,
    SafeJson(input): SafeJson<Revoke>,
) -> Response {
    let (actor, id) = match revocation(&headers, input) {
        Ok(pair) => pair,
        Err(e) => return failure(e),
    };
    response(service.store.revoke_invitation(actor, id).await)
}
fn revocation(headers: &HeaderMap, input: Revoke) -> Result<([u8; 32], InvitationId), Error> {
    let id = uuid::Uuid::parse_str(&input.id).map_err(|_| Error::Invalid)?;
    Ok((
        actor(headers)?,
        InvitationId::from_u128(id.as_u128()).map_err(|_| Error::Invalid)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Accept {
    token: String,
    email: String,
    first_name: String,
    last_name: String,
    password: String,
}
async fn accept<S: InvitationStore, K, P: PasswordPreparation>(
    State(service): State<Arc<Service<S, K, P>>>,
    SafeJson(input): SafeJson<Accept>,
) -> Response {
    let token = zeroize::Zeroizing::new(input.token);
    let password = zeroize::Zeroizing::new(input.password);
    let digest = match invitation_digest(&token) {
        Ok(digest) => digest,
        Err(e) => return failure(e),
    };
    response(
        application::accept(
            &service.store,
            &service.passwords,
            digest,
            Acceptance {
                email: &input.email,
                first_name: &input.first_name,
                last_name: &input.last_name,
                password: &password,
            },
        )
        .await
        .map(|_| ()),
    )
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
        Error::Invalid => (StatusCode::BAD_REQUEST, "invalid_invitation"),
        Error::Unauthorized => (StatusCode::UNAUTHORIZED, "sign_in_required"),
        Error::Forbidden => (StatusCode::FORBIDDEN, "administrator_required"),
        Error::RecentAuthentication => (StatusCode::FORBIDDEN, "recent_authentication_required"),
        Error::Conflict => (StatusCode::CONFLICT, "account_exists"),
        Error::Throttled => (StatusCode::TOO_MANY_REQUESTS, "invitation_limited"),
        Error::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable"),
    };
    let mut response = (status, Json(json!({"error":name}))).into_response();
    if error == Error::Unauthorized {
        authentication_http::clear_cookie(&mut response);
    }
    response
}
#[cfg(test)]
#[path = "../tests/unit/invitations_http.rs"]
mod tests;
