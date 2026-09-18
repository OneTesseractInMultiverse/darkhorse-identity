//! Administrator-only resource credential lifecycle; never accepts a caller-supplied actor.
use crate::{
    authentication_http,
    json::SafeJson,
    registration_http::{actor, error, input::id},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use darkhorse_application::resource_servers::*;
use darkhorse_domain::{identity::*, registration::RegistrationError};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

pub fn router<S: Registry + 'static>(service: S, origin: url::Url) -> Router {
    let routes = Router::new()
        .route("/api/admin/resource-introspection", post(write::<S>))
        .route(
            "/api/admin/applications/{application}/resources/{resource}/introspection",
            get(read::<S>),
        )
        .with_state(Arc::new(service));
    authentication_http::protect(routes, origin, 2048, 16)
}
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum Input {
    Register {
        application_id: String,
        resource_id: String,
    },
    Rotate {
        application_id: String,
        resource_id: String,
        revision: u64,
        overlap_seconds: u16,
    },
    SetActive {
        application_id: String,
        resource_id: String,
        revision: u64,
        active: bool,
    },
}
impl Input {
    fn command(self) -> Result<Command, RegistrationError> {
        let (application, resource, change) = match self {
            Self::Register {
                application_id,
                resource_id,
            } => (application_id, resource_id, Change::Register),
            Self::Rotate {
                application_id,
                resource_id,
                revision,
                overlap_seconds,
            } => (
                application_id,
                resource_id,
                Change::Rotate {
                    revision,
                    overlap_seconds,
                },
            ),
            Self::SetActive {
                application_id,
                resource_id,
                revision,
                active,
            } => (
                application_id,
                resource_id,
                Change::SetActive { revision, active },
            ),
        };
        Ok(Command {
            target: target(&application, &resource)?,
            change,
        })
    }
}
fn target(application: &str, resource: &str) -> Result<Target, RegistrationError> {
    Ok(Target {
        application: id(application, ApplicationId::from_u128)?,
        resource: id(resource, ResourceId::from_u128)?,
    })
}
async fn write<S: Registry>(
    State(service): State<Arc<S>>,
    headers: HeaderMap,
    SafeJson(input): SafeJson<Input>,
) -> Response {
    let request = actor(&headers).and_then(|actor| Ok((actor, input.command()?)));
    let (actor, command) = match request {
        Ok(request) => request,
        Err(e) => return error(e),
    };
    match service.write(actor, command).await {
        Ok(written) => Json(output(written)).into_response(),
        Err(e) => error(e),
    }
}
async fn read<S: Registry>(
    State(service): State<Arc<S>>,
    headers: HeaderMap,
    Path((application, resource)): Path<(String, String)>,
) -> Response {
    let request = actor(&headers).and_then(|actor| Ok((actor, target(&application, &resource)?)));
    let (actor, target) = match request {
        Ok(request) => request,
        Err(e) => return error(e),
    };
    match service.read(actor, target).await {
        Ok(record) => Json(record_value(record)).into_response(),
        Err(e) => error(e),
    }
}
fn output(written: Written) -> Value {
    let mut value = record_value(written.record);
    if let Some(secret) = written.secret {
        value["secret"] = secret.into();
    }
    value
}
fn record_value(record: Record) -> Value {
    json!({"application_id":uuid::Uuid::from_u128(record.target.application.as_u128()).to_string(),
        "resource_id":uuid::Uuid::from_u128(record.target.resource.as_u128()).to_string(),
        "introspection_client_id":format!("rs_{}",uuid::Uuid::from_u128(record.target.resource.as_u128())),
        "active":record.active,"revision":record.revision,
        "secrets":record.secrets.into_iter().map(|s|json!({"id":uuid::Uuid::from_u128(s.id.as_u128()).to_string(),"created_ms":s.created_ms,"expires_ms":s.expires_ms})).collect::<Vec<_>>()})
}

#[cfg(test)]
#[path = "../../tests/unit/resource_servers_http.rs"]
mod tests;
