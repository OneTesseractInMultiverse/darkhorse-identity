//! Opt-in pending authorization transport; no token issuance or discovery claims yet.
mod request;
mod response;
use crate::{authentication_http, json::SafeJson, session_secret};
use axum::{
    Json, Router,
    extract::{RawQuery, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use darkhorse_application::{authentication::AuthError, oidc::*, signing::SigningStore};
use darkhorse_domain::oidc::Error;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;
const COOKIE: &str = "__Host-darkhorse-authorization";
struct Provider<S> {
    store: S,
    issuer: String,
}
pub fn router<S: AuthorizationStore + SigningStore + 'static>(
    store: S,
    origin: url::Url,
) -> Router {
    let state = Arc::new(Provider {
        store,
        issuer: origin.origin().ascii_serialization(),
    });
    let public = Router::new()
        .route("/authorize", get(begin::<S>))
        .route("/jwks", get(jwks::<S>))
        .route("/.well-known/openid-configuration", get(discovery))
        .with_state(state.clone());
    let private = Router::new()
        .route("/api/authorization", get(inspect::<S>))
        .route("/api/authorization/decision", post(decide::<S>))
        .with_state(state);
    authentication_http::protect_with_queries(public, origin.clone(), 0, 32, true)
        .merge(authentication_http::protect(private, origin, 1024, 32))
}
// A code-flow discovery document must advertise an implemented token endpoint.
async fn discovery() -> Response {
    response::error(Error::Unavailable)
}
async fn jwks<S: SigningStore>(State(state): State<Arc<Provider<S>>>) -> Response {
    match state.store.published(&state.issuer).await {
  Ok(keys)=>Json(serde_json::json!({"keys":keys.into_iter().map(|key|serde_json::json!({"kty":"RSA","use":"sig","alg":"RS256","kid":key.kid,"n":key.n,"e":key.e})).collect::<Vec<_>>()})).into_response(),
  Err(_)=>response::error(Error::Unavailable),
 }
}
async fn begin<S: AuthorizationStore>(
    State(state): State<Arc<Provider<S>>>,
    method: Method,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    if method != Method::GET {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let request = match request::parse(query.as_deref().unwrap_or("")) {
        Ok(request) => request,
        Err(error) => return response::error(error),
    };
    let silent = request.prompt == darkhorse_domain::oidc::Prompt::None;
    let target = ReturnTo {
        uri: request.redirect.clone(),
        state: request.state.clone(),
    };
    let session = match authentication_http::cookie(&headers) {
        Ok(value) => value,
        Err(_) => return response::error(Error::InvalidRequest),
    };
    let (secret, digest) = match handle() {
        Ok(value) => value,
        Err(error) => return response::error(error),
    };
    match state.store.begin(request, digest, session).await {
        Ok(Outcome::Pending(_)) if silent => response::redirect(target, Error::Unavailable),
        Ok(Outcome::Pending(_)) => {
            let mut response = Redirect::to("/authorization").into_response();
            let cookie =
                format!("{COOKIE}={secret}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=300");
            response.headers_mut().insert(
                header::SET_COOKIE,
                HeaderValue::from_str(&cookie).expect("hex cookie"),
            );
            response
        }
        Ok(Outcome::Return { target, error }) => response::redirect(target, error),
        Err(error) => response::error(error),
    }
}
async fn inspect<S: AuthorizationStore>(
    State(state): State<Arc<Provider<S>>>,
    headers: HeaderMap,
) -> Response {
    resume(&state, &headers, Decision::Inspect, None).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Submission {
    request_id: String,
    decision: Choice,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Choice {
    Approve,
    Deny,
}
async fn decide<S: AuthorizationStore>(
    State(state): State<Arc<Provider<S>>>,
    headers: HeaderMap,
    SafeJson(body): SafeJson<Submission>,
) -> Response {
    let decision = match body.decision {
        Choice::Approve => Decision::Approve,
        Choice::Deny => Decision::Deny,
    };
    resume(&state, &headers, decision, Some(&body.request_id)).await
}
async fn resume<S: AuthorizationStore>(
    state: &Provider<S>,
    headers: &HeaderMap,
    decision: Decision,
    confirmation: Option<&str>,
) -> Response {
    let digest = match authentication_http::named_cookie(headers, COOKIE, handle_digest) {
        Ok(Some(value)) => value,
        _ => return response::error(Error::InvalidTransaction),
    };
    if confirmation.is_some_and(|id| id != session_secret::hex(&digest)) {
        return response::error(Error::InvalidTransaction);
    }
    let session = match authentication_http::cookie(headers) {
        Ok(value) => value,
        Err(_) => return response::error(Error::InvalidTransaction),
    };
    match state.store.resume(digest, session, decision).await {
        Ok(Outcome::Pending(view)) => response::view(view, digest),
        Ok(Outcome::Return { target, error }) => response::return_json(target, error),
        Err(error) => response::error(error),
    }
}
fn handle() -> Result<(String, [u8; 32]), Error> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| Error::Unavailable)?;
    let secret = session_secret::hex(&bytes);
    let digest = handle_digest(&secret).map_err(|_| Error::Unavailable)?;
    Ok((secret, digest))
}
fn handle_digest(secret: &str) -> Result<[u8; 32], AuthError> {
    session_secret::decode(secret)?;
    Ok(Sha256::new()
        .chain_update(b"darkhorse:authorization:v1\0")
        .chain_update(secret.as_bytes())
        .finalize()
        .into())
}
#[cfg(test)]
#[path = "../../tests/unit/provider_http/mod.rs"]
mod tests;
