//! Administrator-only registration; saving does not probe arbitrary destinations.
use crate::{
    authentication_http,
    json::SafeJson,
    registration_http::{actor, error, input},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::get,
};
use darkhorse_application::logout::registration::*;
use darkhorse_domain::{
    identity::{ApplicationId, ClientId},
    registration::RegistrationError,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

pub fn router<S: Registration + 'static>(service: S, origin: url::Url) -> Router {
    authentication_http::protect(
        Router::new()
            .route(
                "/api/admin/applications/{application}/clients/{client}/logout",
                get(read::<S>).put(write::<S>),
            )
            .with_state(Arc::new(service)),
        origin,
        8192,
        8,
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    #[serde(deserialize_with = "input::revision")]
    revision: u64,
    backchannel_logout_uri: Option<String>,
    backchannel_logout_session_required: bool,
}
async fn read<S: Registration>(
    State(service): State<Arc<S>>,
    headers: HeaderMap,
    Path((application, client)): Path<(String, String)>,
) -> Response {
    let (actor, target) = match request(&headers, &application, &client) {
        Ok(value) => value,
        Err(e) => return error(e),
    };
    match service.logout_configuration(actor, target).await {
        Ok(value) => Json(record(value)).into_response(),
        Err(e) => error(e),
    }
}
async fn write<S: Registration>(
    State(service): State<Arc<S>>,
    headers: HeaderMap,
    Path((application, client)): Path<(String, String)>,
    SafeJson(body): SafeJson<Input>,
) -> Response {
    let (actor, target) = match request(&headers, &application, &client) {
        Ok(value) => value,
        Err(e) => return error(e),
    };
    let body = match body.configuration() {
        Ok(value) => value,
        Err(e) => return error(e),
    };
    match service.configure_logout(actor, target, body).await {
        Ok(value) => Json(configuration(value)).into_response(),
        Err(e) => error(e),
    }
}
impl Input {
    fn configuration(self) -> Result<Configuration, RegistrationError> {
        if !self.backchannel_logout_session_required {
            return Err(RegistrationError::Invalid);
        }
        if let Some(uri) = &self.backchannel_logout_uri {
            crate::logout::destination(uri)?;
        }
        Ok(Configuration {
            revision: self.revision,
            uri: self.backchannel_logout_uri,
        })
    }
}
fn request(
    headers: &HeaderMap,
    application: &str,
    client: &str,
) -> Result<([u8; 32], Target), RegistrationError> {
    Ok((
        actor(headers)?,
        Target {
            application: input::id(application, ApplicationId::from_u128)?,
            client: input::id(client, ClientId::from_u128)?,
        },
    ))
}
fn configuration(value: Configuration) -> serde_json::Value {
    json!({"revision":value.revision.to_string(),"backchannel_logout_uri":value.uri,"backchannel_logout_session_required":true})
}
fn record(value: Record) -> serde_json::Value {
    json!({
        "configuration": configuration(value.configuration),
        "deliveries": value.deliveries.into_iter().map(delivery).collect::<Vec<_>>()
    })
}
fn delivery(value: DeliveryStatus) -> serde_json::Value {
    json!({
        "id": uuid::Uuid::from_u128(value.id.as_u128()).to_string(),
        "state": value.state,
        "attempts": value.attempts,
        "created_ms": value.created_ms
    })
}

#[cfg(test)]
#[path = "../tests/unit/logout_http.rs"]
mod tests;
