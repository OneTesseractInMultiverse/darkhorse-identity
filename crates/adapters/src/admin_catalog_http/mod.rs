//! Console catalog boundary. Every service authenticates current primary authority.
use crate::{
    authentication_http,
    json::SafeJson,
    registration_http::{actor, error, input::id},
};
use axum::{
    Json, Router,
    extract::{Path, RawQuery, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use darkhorse_application::{
    admin_catalog::*,
    registration::{ReadTarget, Registration},
};
use darkhorse_domain::{identity::*, registration::RegistrationError as Error};
use serde_json::{Value, json};
use std::sync::Arc;
mod input;
mod output;
struct Services<C, R> {
    catalog: C,
    registration: R,
}
pub fn router<C: Catalog + 'static, R: Registration + 'static>(
    catalog: C,
    registration: R,
    origin: url::Url,
) -> Router {
    let state = Arc::new(Services {
        catalog,
        registration,
    });
    let reads = Router::new()
        .route("/api/admin/catalog/{kind}", get(list::<C, R>))
        .route("/api/admin/catalog/{kind}/{id}", get(view::<C, R>))
        .with_state(state.clone());
    let registration = Router::new()
        .route("/api/admin/console/registration", post(register::<C, R>))
        .route(
            "/api/admin/console/applications/{app}",
            get(application::<C, R>),
        )
        .route(
            "/api/admin/console/applications/{app}/clients/{client}",
            get(client::<C, R>),
        )
        .route("/api/admin/catalog", post(write::<C, R>))
        .with_state(state);
    authentication_http::protect_with_queries(reads, origin.clone(), 32768, 16, true).merge(
        authentication_http::protect(registration, origin, 32768, 16),
    )
}
async fn list<C: Catalog, R: Registration>(
    State(s): State<Arc<Services<C, R>>>,
    headers: HeaderMap,
    Path(kind): Path<String>,
    RawQuery(raw): RawQuery,
) -> Response {
    let (actor, (target, q)) =
        match actor(&headers).and_then(|a| Ok((a, input::list(&kind, raw.as_deref())?))) {
            Ok(v) => v,
            Err(e) => return error(e),
        };
    respond(s.catalog.list(actor, target, q).await.map(output::page))
}
async fn view<C: Catalog, R: Registration>(
    State(s): State<Arc<Services<C, R>>>,
    headers: HeaderMap,
    Path((kind, id)): Path<(String, String)>,
    RawQuery(raw): RawQuery,
) -> Response {
    let (actor, target) =
        match actor(&headers).and_then(|a| Ok((a, input::target(&kind, &id, raw.as_deref())?))) {
            Ok(v) => v,
            Err(e) => return error(e),
        };
    respond(s.catalog.view(actor, target).await.map(output::view))
}
async fn write<C: Catalog, R: Registration>(
    State(s): State<Arc<Services<C, R>>>,
    headers: HeaderMap,
    SafeJson(input): SafeJson<input::Mutation>,
) -> Response {
    let (actor, change) = match actor(&headers).and_then(|a| Ok((a, input.change.change()?))) {
        Ok(v) => v,
        Err(e) => return error(e),
    };
    respond(
        s.catalog
            .write(actor, input.policy_revision, change)
            .await
            .map(output::written),
    )
}
async fn register<C: Catalog, R: Registration>(
    State(s): State<Arc<Services<C, R>>>,
    headers: HeaderMap,
    SafeJson(input): SafeJson<crate::registration_http::input::Input>,
) -> Response {
    let (actor, command) = match actor(&headers).and_then(|a| Ok((a, input.command()?))) {
        Ok(v) => v,
        Err(e) => return error(e),
    };
    respond(
        s.registration
            .write(actor, command)
            .await
            .map(output::registered),
    )
}
async fn application<C: Catalog, R: Registration>(
    State(s): State<Arc<Services<C, R>>>,
    headers: HeaderMap,
    Path(app): Path<String>,
) -> Response {
    let (actor, target) = match actor(&headers).and_then(|a| {
        Ok((
            a,
            ReadTarget::Application(id(&app, ApplicationId::from_u128)?),
        ))
    }) {
        Ok(v) => v,
        Err(e) => return error(e),
    };
    respond(
        s.registration
            .read(actor, target)
            .await
            .map(output::registration),
    )
}
async fn client<C: Catalog, R: Registration>(
    State(s): State<Arc<Services<C, R>>>,
    headers: HeaderMap,
    Path((app, client)): Path<(String, String)>,
) -> Response {
    let (actor, target) = match actor(&headers).and_then(|a| {
        Ok((
            a,
            ReadTarget::Client {
                application: id(&app, ApplicationId::from_u128)?,
                client: id(&client, ClientId::from_u128)?,
            },
        ))
    }) {
        Ok(v) => v,
        Err(e) => return error(e),
    };
    respond(
        s.registration
            .read(actor, target)
            .await
            .map(output::registration),
    )
}
fn respond(result: Result<Value, Error>) -> Response {
    match result {
        Ok(value) => Json(value).into_response(),
        Err(e) => error(e),
    }
}
#[cfg(test)]
#[path = "../../tests/unit/admin_catalog_http.rs"]
mod tests;
