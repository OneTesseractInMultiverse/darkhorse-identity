mod input;
use crate::{
    authentication_http,
    tokens::material::{self, Purpose},
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use darkhorse_application::{signing::SigningStore, tokens::*};
use darkhorse_domain::{signing::Phase, tokens::Error};
use std::sync::Arc;
struct Endpoint<S, K> {
    store: S,
    signer: K,
    issuer: String,
}
pub fn router<S: TokenStore + SigningStore + 'static, K: IdSigner + 'static>(
    store: S,
    signer: K,
    origin: url::Url,
) -> Router {
    let endpoint = Arc::new(Endpoint {
        store,
        signer,
        issuer: origin.origin().ascii_serialization(),
    });
    let routes = Router::new()
        .route("/token", post(redeem::<S, K>))
        .route("/userinfo", get(userinfo::<S, K>))
        .route("/.well-known/openid-configuration", get(discovery::<S, K>))
        .with_state(endpoint);
    authentication_http::protect_service(routes, origin, 4096, 16)
        .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::PRAGMA,
            HeaderValue::from_static("no-cache"),
        ))
}
async fn redeem<S: TokenStore, K: IdSigner>(
    State(e): State<Arc<Endpoint<S, K>>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let input = match input::request(&headers, &body) {
        Ok(input) => input,
        Err(error) => return failure(error),
    };
    let access = match material::generate(Purpose::Access) {
        Ok(value) => value,
        Err(error) => return failure(error),
    };
    match e.store.redeem(input, access, &e.issuer, &e.signer).await {
        Ok(tokens) => token_response(tokens),
        Err(error) => failure(error),
    }
}
fn token_response(tokens: Tokens) -> Response {
    Json(serde_json::json!({
        "access_token": tokens.access,
        "token_type": "Bearer",
        "expires_in": tokens.expires_in,
        "id_token": tokens.id_token,
        "scope": "openid"
    }))
    .into_response()
}
async fn userinfo<S: TokenStore, K>(
    State(e): State<Arc<Endpoint<S, K>>>,
    headers: HeaderMap,
) -> Response {
    let digest = match input::bearer(&headers) {
        Ok(value) => value,
        Err(error) => return failure(error),
    };
    match e.store.userinfo(digest, &e.issuer).await {
        Ok(subject) => {
            Json(serde_json::json!({"sub":uuid::Uuid::from_u128(subject.as_u128()).to_string()}))
                .into_response()
        }
        Err(error) => failure(error),
    }
}
async fn discovery<S: SigningStore, K>(State(e): State<Arc<Endpoint<S, K>>>) -> Response {
    match e.store.inventory(&e.issuer).await {
        Ok(inventory)
            if inventory
                .keys
                .iter()
                .any(|key| key.state.phase == Phase::Active) =>
        {
            Json(metadata(&e.issuer)).into_response()
        }
        _ => failure(Error::Unavailable),
    }
}
fn metadata(issuer: &str) -> serde_json::Value {
    serde_json::json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/authorize"),
        "token_endpoint": format!("{issuer}/token"),
        "userinfo_endpoint": format!("{issuer}/userinfo"),
        "jwks_uri": format!("{issuer}/jwks"),
        "response_types_supported": ["code"],
        "response_modes_supported": ["query"],
        "grant_types_supported": ["authorization_code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
        "token_endpoint_auth_methods_supported": ["client_secret_basic"],
        "code_challenge_methods_supported": ["S256"],
        "scopes_supported": ["openid"],
        "claims_supported": ["iss", "sub", "aud", "exp", "iat", "auth_time", "nonce"],
        "claims_parameter_supported": false,
        "request_parameter_supported": false,
        "request_uri_parameter_supported": false,
        "authorization_response_iss_parameter_supported": true
    })
}
fn failure(error: Error) -> Response {
    let (status, name) = match error {
        Error::InvalidRequest => (StatusCode::BAD_REQUEST, "invalid_request"),
        Error::InvalidClient => (StatusCode::UNAUTHORIZED, "invalid_client"),
        Error::InvalidGrant => (StatusCode::BAD_REQUEST, "invalid_grant"),
        Error::InvalidScope => (StatusCode::BAD_REQUEST, "invalid_scope"),
        Error::InvalidToken => (StatusCode::UNAUTHORIZED, "invalid_token"),
        Error::UnsupportedGrant => (StatusCode::BAD_REQUEST, "unsupported_grant_type"),
        Error::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable"),
    };
    let mut response = (status, Json(serde_json::json!({"error":name}))).into_response();
    if error == Error::InvalidClient {
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Basic realm=\"token\""),
        );
    }
    if error == Error::InvalidToken {
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Bearer error=\"invalid_token\""),
        );
    }
    response
}
