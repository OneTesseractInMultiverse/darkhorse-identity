//! Same-origin, original-session profile access. Transport DTOs have an explicit allowlist.
use crate::{authentication_http, json::SafeJson};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use darkhorse_application::profiles::{Profile, Store};
use darkhorse_domain::{
    identity::PrincipalId,
    profiles::{Error, Fields, Phone},
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
pub(crate) mod input;
use input::{Input, prepare, target};
pub fn router<S: Store + 'static>(store: S, origin: url::Url) -> Router {
    let routes = Router::new()
        .route("/api/profiles/options", get(options::<S>))
        .route("/api/profiles/{target}", get(read::<S>).post(update::<S>))
        .with_state(Arc::new(store));
    authentication_http::protect(routes, origin, 16384, 16).layer(
        tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ),
    )
}
pub(crate) fn actor(headers: &HeaderMap) -> Result<[u8; 32], Error> {
    authentication_http::cookie(headers)
        .map_err(|_| Error::Unauthorized)?
        .ok_or(Error::Unauthorized)
}
async fn options<S: Store>(State(store): State<Arc<S>>, headers: HeaderMap) -> Response {
    let actor = match actor(&headers) {
        Ok(actor) => actor,
        Err(e) => return failure(e),
    };
    match store.profile(actor, None).await {
        Err(error) => failure(error),
        Ok(_) => Json(project_options()).into_response(),
    }
}
fn project_options() -> serde_json::Value {
    json!({
        "country_version": crate::profiles::COUNTRY_VERSION,
        "phone_version": crate::profiles::PHONE_VERSION,
        "countries": crate::profiles::countries().into_iter()
            .map(|(code, name)| json!({ "code": code, "name": name })).collect::<Vec<_>>(),
        "calling_codes": crate::profiles::calling_codes().into_iter()
            .map(|code| code.to_string()).collect::<Vec<_>>()
    })
}
async fn read<S: Store>(
    State(store): State<Arc<S>>,
    headers: HeaderMap,
    Path(value): Path<String>,
) -> Response {
    let (actor, target) =
        match actor(&headers).and_then(|actor| target(&value).map(|target| (actor, target))) {
            Ok(input) => input,
            Err(e) => return failure(e),
        };
    respond(store.profile(actor, target).await)
}
async fn update<S: Store>(
    State(store): State<Arc<S>>,
    headers: HeaderMap,
    Path(value): Path<String>,
    SafeJson(input): SafeJson<Input>,
) -> Response {
    let parsed = actor(&headers).and_then(|actor| Ok((actor, target(&value)?, prepare(input)?)));
    let (actor, target, (revision, fields)) = match parsed {
        Ok(input) => input,
        Err(e) => return failure(e),
    };
    respond(store.update_profile(actor, target, revision, fields).await)
}
fn respond(result: Result<Profile, Error>) -> Response {
    match result {
        Ok(p) => Json(project(p)).into_response(),
        Err(e) => failure(e),
    }
}
fn project(p: Profile) -> serde_json::Value {
    let f = p.fields;
    json!({
        "id": uuid::Uuid::from_u128(p.id.as_u128()).to_string(),
        "email": p.email,
        "active": p.active,
        "email_verified": p.email_verified,
        "revision": p.revision.to_string(),
        "first_name": f.first_name(),
        "second_name": f.second_name().unwrap_or(""),
        "last_name": f.last_name(),
        "second_last_name": f.second_last_name().unwrap_or(""),
        "country": f.country().unwrap_or(""),
        "bio": f.bio().unwrap_or(""),
        "calling_code": f.phone().map(Phone::calling_code).unwrap_or(""),
        "national_number": f.phone().map(Phone::national_number).unwrap_or("")
    })
}
pub(crate) fn failure(error: Error) -> Response {
    let (status, name) = match error {
        Error::Invalid => (StatusCode::BAD_REQUEST, "invalid_request"),
        Error::Unauthorized => (StatusCode::UNAUTHORIZED, "sign_in_required"),
        Error::Forbidden => (StatusCode::FORBIDDEN, "access_denied"),
        Error::RecentAuthentication => (StatusCode::FORBIDDEN, "recent_authentication_required"),
        Error::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        Error::Conflict => (StatusCode::CONFLICT, "revision_conflict"),
        Error::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable"),
    };
    let mut response = (status, Json(json!({"error":name}))).into_response();
    if error == Error::Unauthorized {
        authentication_http::clear_cookie(&mut response);
    }
    response
}
#[cfg(test)]
#[path = "../../tests/unit/profiles_http/mod.rs"]
mod tests;
