//! Same-origin, bounded self-service management for personal API keys.
mod input;
mod output;
use crate::{authentication_http, json::SafeJson};
use axum::{
    Json, Router,
    extract::{RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use darkhorse_application::personal_keys::{Entropy, Service, Store};
use darkhorse_domain::{identity::*, personal_keys::Error};
use std::sync::Arc;
pub fn router<S: Store + 'static, E: Entropy + 'static>(
    service: Service<S, E>,
    origin: url::Url,
) -> Router {
    let state = Arc::new(service);
    let read = Router::new()
        .route("/api/security/keys", get(list::<S, E>))
        .route("/api/security/keys/options", get(options::<S, E>))
        .with_state(state.clone());
    let write = Router::new()
        .route("/api/security/keys", post(create::<S, E>))
        .route("/api/security/keys/revoke", post(revoke::<S, E>))
        .with_state(state);
    authentication_http::protect_with_queries(read, origin.clone(), 1024, 8, true)
        .merge(authentication_http::protect(write, origin, 262144, 8))
        .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
}
async fn list<S: Store, E: Entropy>(
    State(service): State<Arc<Service<S, E>>>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let (actor, after) = match read_input(&headers, query.as_deref()) {
        Ok(input) => input,
        Err(e) => return failure(e),
    };
    let after = after
        .map(CredentialId::from_u128)
        .transpose()
        .map_err(|_| Error::Invalid);
    let after = match after {
        Ok(value) => value,
        Err(e) => return failure(e),
    };
    match service.store.list(actor, after).await {
        Ok(page) => Json(output::page(page)).into_response(),
        Err(e) => failure(e),
    }
}
async fn options<S: Store, E: Entropy>(
    State(service): State<Arc<Service<S, E>>>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let (actor, after) = match read_input(&headers, query.as_deref()) {
        Ok(input) => input,
        Err(e) => return failure(e),
    };
    let after = after
        .map(ResourceId::from_u128)
        .transpose()
        .map_err(|_| Error::Invalid);
    let after = match after {
        Ok(value) => value,
        Err(e) => return failure(e),
    };
    match service.store.options(actor, after).await {
        Ok(page) => Json(output::options(page)).into_response(),
        Err(e) => failure(e),
    }
}
fn read_input(headers: &HeaderMap, query: Option<&str>) -> Result<([u8; 32], Option<u128>), Error> {
    Ok((actor(headers)?, input::cursor(query)?))
}
fn actor(headers: &HeaderMap) -> Result<[u8; 32], Error> {
    authentication_http::cookie(headers)
        .map_err(|_| Error::Unauthorized)?
        .ok_or(Error::Unauthorized)
}
async fn create<S: Store, E: Entropy>(
    State(service): State<Arc<Service<S, E>>>,
    headers: HeaderMap,
    SafeJson(input): SafeJson<input::Creation>,
) -> Response {
    let (actor, request) = match creation(&headers, input) {
        Ok(value) => value,
        Err(e) => return failure(e),
    };
    match service.create(actor, &request).await {
        Ok(created) => (
            StatusCode::CREATED,
            Json(serde_json::json!({"key":output::record(created.record),"secret":created.secret})),
        )
            .into_response(),
        Err(e) => failure(e),
    }
}
fn creation(
    headers: &HeaderMap,
    input: input::Creation,
) -> Result<([u8; 32], darkhorse_domain::personal_keys::Request), Error> {
    Ok((actor(headers)?, input::request(input)?))
}
async fn revoke<S: Store, E: Entropy>(
    State(service): State<Arc<Service<S, E>>>,
    headers: HeaderMap,
    SafeJson(input): SafeJson<input::Revocation>,
) -> Response {
    let (actor, id) = match revocation(&headers, input) {
        Ok(value) => value,
        Err(e) => return failure(e),
    };
    match service.store.revoke(actor, id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => failure(e),
    }
}
fn revocation(
    headers: &HeaderMap,
    input: input::Revocation,
) -> Result<([u8; 32], CredentialId), Error> {
    Ok((
        actor(headers)?,
        CredentialId::from_u128(input::identifier(&input.key_id)?).map_err(|_| Error::Invalid)?,
    ))
}
fn failure(error: Error) -> Response {
    let (status, name) = match error {
        Error::Invalid => (StatusCode::BAD_REQUEST, "invalid_request"),
        Error::Unauthorized => (StatusCode::UNAUTHORIZED, "sign_in_required"),
        Error::Forbidden => (StatusCode::FORBIDDEN, "access_denied"),
        Error::RecentAuthenticationRequired => {
            (StatusCode::FORBIDDEN, "recent_authentication_required")
        }
        Error::NotFound => (StatusCode::NOT_FOUND, "key_not_found"),
        Error::Conflict => (StatusCode::CONFLICT, "policy_changed"),
        Error::Limit => (StatusCode::TOO_MANY_REQUESTS, "key_limit"),
        Error::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable"),
    };
    let mut response = (status, Json(serde_json::json!({"error":name}))).into_response();
    if error == Error::Unauthorized {
        authentication_http::clear_cookie(&mut response);
    }
    response
}
#[cfg(test)]
#[path = "../../tests/unit/personal_keys_http/mod.rs"]
mod tests;
