//! Private administrator API, protected by the browser session and same-origin boundary.
use crate::{authentication_http, json::SafeJson};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use darkhorse_application::registration::*;
use darkhorse_domain::{identity::*, registration::*};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
mod input;
mod output;
pub fn router<S: Registration + 'static>(service: S, origin: url::Url) -> Router {
    let router = Router::new()
        .route("/api/admin/registration", post(write::<S>))
        .route(
            "/api/admin/applications/{application}",
            get(application::<S>),
        )
        .route(
            "/api/admin/applications/{application}/clients/{client}",
            get(client::<S>),
        )
        .with_state(Arc::new(service));
    authentication_http::protect(router, origin, 32768, 16)
}
async fn write<S: Registration>(
    State(service): State<Arc<S>>,
    headers: HeaderMap,
    SafeJson(body): SafeJson<input::Input>,
) -> Response {
    let (actor, command) = match request(&headers, body) {
        Ok(value) => value,
        Err(e) => return error(e),
    };
    match service.write(actor, command).await {
        Ok(written) => Json(output::written(written)).into_response(),
        Err(e) => error(e),
    }
}
fn request(
    headers: &HeaderMap,
    body: input::Input,
) -> Result<([u8; 32], Command), RegistrationError> {
    Ok((actor(headers)?, body.command()?))
}
async fn application<S: Registration>(
    State(service): State<Arc<S>>,
    headers: HeaderMap,
    Path(application): Path<String>,
) -> Response {
    let target = input::id(&application, ApplicationId::from_u128).map(ReadTarget::Application);
    read(&*service, &headers, target).await
}
async fn client<S: Registration>(
    State(service): State<Arc<S>>,
    headers: HeaderMap,
    Path((application, client)): Path<(String, String)>,
) -> Response {
    read(&*service, &headers, client_target(&application, &client)).await
}
fn client_target(application: &str, client: &str) -> Result<ReadTarget, RegistrationError> {
    Ok(ReadTarget::Client {
        application: input::id(application, ApplicationId::from_u128)?,
        client: input::id(client, ClientId::from_u128)?,
    })
}
async fn read(
    service: &impl Registration,
    headers: &HeaderMap,
    target: Result<ReadTarget, RegistrationError>,
) -> Response {
    let actor = match actor(headers) {
        Ok(actor) => actor,
        Err(e) => return error(e),
    };
    let target = match target {
        Ok(target) => target,
        Err(e) => return error(e),
    };
    match service.read(actor, target).await {
        Ok(record) => Json(output::record(record)).into_response(),
        Err(e) => error(e),
    }
}
fn actor(headers: &HeaderMap) -> Result<[u8; 32], RegistrationError> {
    authentication_http::cookie(headers)
        .ok()
        .flatten()
        .ok_or(RegistrationError::Unauthorized)
}
fn error(error: RegistrationError) -> Response {
    let (status, code) = match error {
        RegistrationError::Invalid => (StatusCode::BAD_REQUEST, "invalid_registration"),
        RegistrationError::Unauthorized => (StatusCode::UNAUTHORIZED, "authentication_required"),
        RegistrationError::Forbidden => (StatusCode::FORBIDDEN, "administrator_required"),
        RegistrationError::RecentAuthenticationRequired => {
            (StatusCode::FORBIDDEN, "recent_authentication_required")
        }
        RegistrationError::NotFound => (StatusCode::NOT_FOUND, "registration_not_found"),
        RegistrationError::Conflict => (StatusCode::CONFLICT, "registration_conflict"),
        RegistrationError::Unavailable => {
            (StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable")
        }
    };
    (status, Json(json!({"error":code}))).into_response()
}
#[cfg(test)]
#[path = "../../tests/unit/registration_http.rs"]
mod tests;
