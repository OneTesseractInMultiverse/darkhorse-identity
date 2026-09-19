//! Bounded, same-origin self-service session management.
use crate::{authentication_http, json::SafeJson};
use axum::{
    Json, Router,
    extract::{RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use darkhorse_application::sessions::{Ended, SessionManagement};
use darkhorse_domain::{
    identity::SessionId,
    sessions::{Cursor, Error, Page, Status},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

pub fn router<S: SessionManagement + 'static>(service: S, origin: url::Url) -> Router {
    let state = Arc::new(service);
    let read = Router::new()
        .route("/api/security/sessions", get(list::<S>))
        .with_state(state.clone());
    let write = Router::new()
        .route("/api/security/sessions/end", post(end::<S>))
        .with_state(state);
    authentication_http::protect_with_queries(read, origin.clone(), 1024, 16, true)
        .merge(authentication_http::protect(write, origin, 1024, 16))
        .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
}
async fn list<S: SessionManagement>(
    State(service): State<Arc<S>>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let (actor, after) = match list_input(&headers, query.as_deref()) {
        Ok(input) => input,
        Err(e) => return failure(e),
    };
    match service.sessions(actor, after).await {
        Ok(page) => Json(projection(page)).into_response(),
        Err(e) => failure(e),
    }
}
fn list_input(
    headers: &HeaderMap,
    query: Option<&str>,
) -> Result<([u8; 32], Option<Cursor>), Error> {
    Ok((actor(headers)?, cursor(query)?))
}
fn actor(headers: &HeaderMap) -> Result<[u8; 32], Error> {
    authentication_http::cookie(headers)
        .map_err(|_| Error::Unauthorized)?
        .ok_or(Error::Unauthorized)
}
fn cursor(query: Option<&str>) -> Result<Option<Cursor>, Error> {
    let Some(query) = query else {
        return Ok(None);
    };
    if query.len() > 128 {
        return Err(Error::Invalid);
    }
    let mut fields = url::form_urlencoded::parse(query.as_bytes());
    let (key, value) = fields.next().ok_or(Error::Invalid)?;
    if key != "after" || fields.next().is_some() {
        return Err(Error::Invalid);
    }
    let (timestamp, reference) = value.split_once(':').ok_or(Error::Invalid)?;
    let created = timestamp.parse::<u64>().map_err(|_| Error::Invalid)?;
    if timestamp != created.to_string() {
        return Err(Error::Invalid);
    }
    Cursor::new(created, id(reference)?).map(Some)
}
fn id(value: &str) -> Result<SessionId, Error> {
    let value_id = uuid::Uuid::parse_str(value).map_err(|_| Error::Invalid)?;
    if value_id.to_string() != value {
        return Err(Error::Invalid);
    }
    SessionId::from_u128(value_id.as_u128()).map_err(|_| Error::Invalid)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    session_id: String,
}
async fn end<S: SessionManagement>(
    State(service): State<Arc<S>>,
    headers: HeaderMap,
    SafeJson(input): SafeJson<Input>,
) -> Response {
    let (actor, target) = match end_input(&headers, input) {
        Ok(input) => input,
        Err(e) => return failure(e),
    };
    match service.end_session(actor, target).await {
        Ok(ended) => ended_response(ended),
        Err(e) => failure(e),
    }
}
fn end_input(headers: &HeaderMap, input: Input) -> Result<([u8; 32], SessionId), Error> {
    Ok((actor(headers)?, id(&input.session_id)?))
}
fn ended_response(ended: Ended) -> Response {
    let mut response = Json(json!({"current":ended.current})).into_response();
    if ended.current {
        authentication_http::clear_cookie(&mut response);
    }
    response
}
fn projection(page: Page) -> Value {
    json!({"current":uuid::Uuid::from_u128(page.current.as_u128()).to_string(),"items":page.items.into_iter().map(|r| json!({"id":uuid::Uuid::from_u128(r.id.as_u128()).to_string(),"created_ms":r.created_ms,"seen_ms":r.seen_ms,"expires_ms":r.expires_ms,"status":match r.status {Status::Active=>"active",Status::Inactive=>"inactive"}})).collect::<Vec<_>>(),"next":page.next.map(|c|format!("{}:{}",c.created_ms,uuid::Uuid::from_u128(c.id.as_u128())))})
}
fn failure(error: Error) -> Response {
    let (status, name) = match error {
        Error::Invalid => (StatusCode::BAD_REQUEST, "invalid_request"),
        Error::Unauthorized => (StatusCode::UNAUTHORIZED, "sign_in_required"),
        Error::NotFound => (StatusCode::NOT_FOUND, "session_not_found"),
        Error::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable"),
    };
    let mut response = (status, Json(json!({"error":name}))).into_response();
    if error == Error::Unauthorized {
        authentication_http::clear_cookie(&mut response);
    }
    response
}
#[cfg(test)]
#[path = "../tests/unit/sessions_http.rs"]
mod tests;
