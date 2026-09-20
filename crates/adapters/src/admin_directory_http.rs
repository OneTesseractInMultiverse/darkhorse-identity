//! First-party administrator boundary; UI navigation never grants authority.
use crate::{authentication_http, directory_query, json::SafeJson};
use axum::{
    Json, Router,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use darkhorse_application::admin_directory::*;
use darkhorse_domain::{
    AccountStatus,
    admin_directory::{Change, Error, Names},
    identity::*,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
pub fn router<S: AdminDirectory + 'static>(service: S, origin: url::Url) -> Router {
    let state = Arc::new(service);
    let read = Router::new()
        .route("/api/admin/users", get(list::<S>))
        .route("/api/admin/users/{id}", get(detail::<S>))
        .route("/api/admin/users/{id}/access", get(access::<S>))
        .with_state(state.clone());
    let write = Router::new()
        .route("/api/admin/users/{id}", post(update::<S>))
        .with_state(state);
    authentication_http::protect_with_queries(read, origin.clone(), 4096, 16, true)
        .merge(authentication_http::protect(write, origin, 4096, 16))
        .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
}
fn actor(headers: &HeaderMap) -> Result<[u8; 32], Error> {
    authentication_http::cookie(headers)
        .map_err(|_| Error::Unauthorized)?
        .ok_or(Error::Unauthorized)
}
fn principal(value: &str) -> Result<PrincipalId, Error> {
    directory_query::principal(value).map_err(|_| Error::Invalid)
}
fn reference<T>(
    value: &str,
    create: impl FnOnce(u128) -> Result<T, darkhorse_domain::identity::InvalidIdentifier>,
) -> Result<T, Error> {
    let id = uuid::Uuid::parse_str(value).map_err(|_| Error::Invalid)?;
    if id.to_string() != value {
        return Err(Error::Invalid);
    }
    create(id.as_u128()).map_err(|_| Error::Invalid)
}
async fn list<S: AdminDirectory>(
    State(service): State<Arc<S>>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let input = actor(&headers).and_then(|a| {
        directory_query::parse_admin(query.as_deref().unwrap_or(""))
            .map(|q| (a, q))
            .map_err(|_| Error::Invalid)
    });
    let (actor, query) = match input {
        Ok(v) => v,
        Err(e) => return failure(e),
    };
    match service.users(actor,query).await {
        Ok(page)=>Json(json!({"actor":id(page.actor.as_u128()),"items":page.items.into_iter().map(user).collect::<Vec<_>>(),"next":page.next.map(|n|id(n.as_u128()))})).into_response(),
        Err(e)=>failure(e),
    }
}
async fn detail<S: AdminDirectory>(
    State(service): State<Arc<S>>,
    headers: HeaderMap,
    Path(target): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let input = actor(&headers).and_then(|a| principal(&target).map(|id| (a, id)));
    let (actor, target) = match input {
        Ok(v) if query.is_none() => v,
        Ok(_) => return failure(Error::Invalid),
        Err(e) => return failure(e),
    };
    respond(service.user(actor, target).await)
}
fn selected(query: Option<&str>) -> Result<Option<ApplicationId>, Error> {
    let Some(raw) = query else {
        return Ok(None);
    };
    if raw.len() > 100 {
        return Err(Error::Invalid);
    }
    let fields = url::form_urlencoded::parse(raw.as_bytes()).collect::<Vec<_>>();
    if fields.len() != 1 || fields[0].0 != "application" {
        return Err(Error::Invalid);
    }
    reference(&fields[0].1, ApplicationId::from_u128).map(Some)
}
async fn access<S: AdminDirectory>(
    State(service): State<Arc<S>>,
    headers: HeaderMap,
    Path(target): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let input =
        actor(&headers).and_then(|a| Ok((a, principal(&target)?, selected(query.as_deref())?)));
    let (actor, target, application) = match input {
        Ok(v) => v,
        Err(e) => return failure(e),
    };
    match service.access(actor,target,application).await {
        Ok(view)=>Json(json!({"user":user(view.user),"policy_revision":view.policy_revision.to_string(),"selected":view.selected.map(|s|id(s.as_u128())),"applications":view.applications.into_iter().map(|a|json!({"id":id(a.id.as_u128()),"name":a.name,"active":a.active})).collect::<Vec<_>>(),"roles":view.roles.into_iter().map(|r|json!({"id":id(r.id.as_u128()),"name":r.name,"assigned":r.assigned})).collect::<Vec<_>>()})).into_response(),Err(e)=>failure(e),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    revision: String,
    change: InputChange,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum InputChange {
    Names {
        first_name: String,
        last_name: String,
    },
    Status {
        active: bool,
    },
    Role {
        application_id: String,
        role_id: String,
        assigned: bool,
        policy_revision: String,
    },
}
fn counter(value: &str) -> Result<u64, Error> {
    let n = value.parse::<u64>().map_err(|_| Error::Invalid)?;
    if n.to_string() != value || n > i64::MAX as u64 {
        return Err(Error::Invalid);
    }
    Ok(n)
}
fn change(input: InputChange) -> Result<Change, Error> {
    Ok(match input {
        InputChange::Names {
            first_name,
            last_name,
        } => Change::Names(Names::new(&first_name, &last_name)?),
        InputChange::Status { active } => Change::Status(if active {
            AccountStatus::Active
        } else {
            AccountStatus::Inactive
        }),
        InputChange::Role {
            application_id,
            role_id,
            assigned,
            policy_revision,
        } => Change::Role {
            application: reference(&application_id, ApplicationId::from_u128)?,
            role: reference(&role_id, RoleId::from_u128)?,
            assigned,
            policy_revision: counter(&policy_revision)?,
        },
    })
}
async fn update<S: AdminDirectory>(
    State(service): State<Arc<S>>,
    headers: HeaderMap,
    Path(target): Path<String>,
    SafeJson(input): SafeJson<Input>,
) -> Response {
    let input = actor(&headers).and_then(|a| {
        Ok((
            a,
            principal(&target)?,
            counter(&input.revision)?,
            change(input.change)?,
        ))
    });
    let (actor, target, revision, change) = match input {
        Ok(v) => v,
        Err(e) => return failure(e),
    };
    respond(service.update(actor, target, revision, change).await)
}
fn respond(result: Result<User, Error>) -> Response {
    match result {
        Ok(record) => Json(user(record)).into_response(),
        Err(e) => failure(e),
    }
}
fn id(value: u128) -> String {
    uuid::Uuid::from_u128(value).to_string()
}
fn user(u: User) -> Value {
    json!({"id":id(u.id.as_u128()),"email":u.email,"first_name":u.first_name,"last_name":u.last_name,"active":u.status==AccountStatus::Active,"administrator":u.administrator,"email_verified":u.email_verified,"revision":u.revision.to_string()})
}
fn failure(error: Error) -> Response {
    let (status, name) = match error {
        Error::Invalid => (StatusCode::BAD_REQUEST, "invalid_request"),
        Error::Unauthorized => (StatusCode::UNAUTHORIZED, "sign_in_required"),
        Error::Forbidden => (StatusCode::FORBIDDEN, "administrator_required"),
        Error::RecentAuthentication => (StatusCode::FORBIDDEN, "recent_authentication_required"),
        Error::NotFound => (StatusCode::NOT_FOUND, "user_not_found"),
        Error::Conflict => (StatusCode::CONFLICT, "revision_conflict"),
        Error::LastAdministrator => (StatusCode::CONFLICT, "last_administrator"),
        Error::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable"),
    };
    let mut response = (status, Json(json!({"error":name}))).into_response();
    if error == Error::Unauthorized {
        authentication_http::clear_cookie(&mut response);
    }
    response
}
#[cfg(test)]
#[path = "../tests/unit/admin_directory_http.rs"]
mod tests;
