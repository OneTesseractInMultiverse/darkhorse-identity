//! Same-origin browser transport. Forwarding headers never establish trust.
use crate::{json::SafeJson, session_secret};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use darkhorse_application::authentication::{
    AuthError, BrowserAuthentication, SessionView, SignedIn,
};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;
use zeroize::Zeroizing;

const COOKIE: &str = "__Host-darkhorse";
struct Boundary {
    origin: String,
    host: String,
    slots: Semaphore,
    queries: bool,
    service: bool,
}
pub fn router<S: BrowserAuthentication + 'static>(service: S, origin: url::Url) -> Router {
    let router = Router::new()
        .route("/api/auth/login", post(login::<S>))
        .route("/api/auth/session", get(session::<S>))
        .route("/api/auth/logout", post(logout::<S>))
        .with_state(Arc::new(service));
    protect(router, origin, 4096, 32)
}
pub(crate) fn protect(router: Router, origin: url::Url, body_limit: usize, slots: usize) -> Router {
    protect_with_queries(router, origin, body_limit, slots, false)
}
pub(crate) fn protect_with_queries(
    router: Router,
    origin: url::Url,
    body_limit: usize,
    slots: usize,
    queries: bool,
) -> Router {
    protected(router, origin, body_limit, slots, queries, false)
}
pub(crate) fn protect_service(
    router: Router,
    origin: url::Url,
    body_limit: usize,
    slots: usize,
) -> Router {
    protected(router, origin, body_limit, slots, false, true)
}
fn protected(
    router: Router,
    origin: url::Url,
    body_limit: usize,
    slots: usize,
    queries: bool,
    service: bool,
) -> Router {
    let boundary = Arc::new(Boundary {
        host: origin[url::Position::BeforeHost..url::Position::AfterPort].into(),
        origin: origin.origin().ascii_serialization(),
        slots: Semaphore::new(slots),
        queries,
        service,
    });
    router
        .layer(DefaultBodyLimit::max(body_limit))
        .layer(middleware::from_fn_with_state(boundary, guard))
}
pub fn disabled_router() -> Router {
    Router::new()
        .route("/api/auth/login", post(unavailable))
        .route("/api/auth/session", get(unavailable))
        .route("/api/auth/logout", post(unavailable))
}
async fn unavailable() -> Response {
    error(AuthError::Unavailable)
}

async fn guard(State(boundary): State<Arc<Boundary>>, request: Request, next: Next) -> Response {
    if !allowed(
        &boundary,
        request.headers(),
        request.method(),
        request.uri().query(),
    ) {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorBody {
                error: "invalid_origin",
            }),
        )
            .into_response();
    }
    let Ok(_permit) = boundary.slots.try_acquire() else {
        return error(AuthError::Unavailable);
    };
    match tokio::time::timeout(Duration::from_secs(10), next.run(request)).await {
        Ok(response) => response,
        Err(_) => error(AuthError::Unavailable),
    }
}
fn allowed(b: &Boundary, headers: &HeaderMap, method: &Method, query: Option<&str>) -> bool {
    if (!b.queries && query.is_some()) || one(headers, "host") != Some(b.host.as_str()) {
        return false;
    }
    if method.is_safe() {
        return true;
    }
    if b.service {
        return !headers.contains_key("origin") && !headers.contains_key("cookie");
    }
    one(headers, "origin") == Some(b.origin.as_str())
        && one(headers, "x-darkhorse-csrf") == Some("1")
        && (!headers.contains_key("sec-fetch-site")
            || one(headers, "sec-fetch-site") == Some("same-origin"))
}
fn one<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let first = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        return None;
    }
    Some(first)
}
pub(crate) fn cookie(headers: &HeaderMap) -> Result<Option<[u8; 32]>, AuthError> {
    named_cookie(headers, COOKIE, session_secret::digest)
}
pub(crate) fn named_cookie(
    headers: &HeaderMap,
    name: &str,
    digest: impl Fn(&str) -> Result<[u8; 32], AuthError>,
) -> Result<Option<[u8; 32]>, AuthError> {
    let mut found = None;
    for value in headers.get_all(header::COOKIE) {
        let value = value.to_str().map_err(|_| AuthError::Denied)?;
        for part in value.split(';') {
            let Some((cookie_name, value)) = part.trim().split_once('=') else {
                return Err(AuthError::Denied);
            };
            if cookie_name == name {
                if found.is_some() {
                    return Err(AuthError::Denied);
                }
                found = Some(digest(value)?);
            }
        }
    }
    Ok(found)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LoginBody {
    email: String,
    password: String,
}
#[derive(Serialize)]
struct ProfileBody {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    preferred_locale: Option<&'static str>,
}
#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
}

async fn login<S: BrowserAuthentication>(
    State(service): State<Arc<S>>,
    headers: HeaderMap,
    SafeJson(body): SafeJson<LoginBody>,
) -> Response {
    let password = Zeroizing::new(body.password);
    let previous = match cookie(&headers) {
        Ok(value) => value,
        Err(e) => return error(e),
    };
    login_response(service.login(&body.email, &password, previous).await)
}
fn login_response(result: Result<SignedIn, AuthError>) -> Response {
    match result {
        Ok(signed) => {
            let mut response = profile(signed.view);
            let value = format!(
                "{COOKIE}={}; Path=/; Secure; HttpOnly; SameSite=Lax",
                signed.secret.value
            );
            match HeaderValue::from_str(&value) {
                Ok(header) => {
                    response.headers_mut().insert(header::SET_COOKIE, header);
                    response
                }
                Err(_) => error(AuthError::Unavailable),
            }
        }
        Err(e) => error(e),
    }
}
async fn session<S: BrowserAuthentication>(
    State(service): State<Arc<S>>,
    headers: HeaderMap,
) -> Response {
    let digest = match cookie(&headers) {
        Ok(Some(value)) => value,
        _ => return error(AuthError::Denied),
    };
    match service.session(digest).await {
        Ok(view) => profile(view),
        Err(e) => error(e),
    }
}
async fn logout<S: BrowserAuthentication>(
    State(service): State<Arc<S>>,
    headers: HeaderMap,
) -> Response {
    let digest = match cookie(&headers) {
        Ok(value) => value,
        Err(e) => return error(e),
    };
    if let Some(digest) = digest
        && let Err(e) = service.logout(digest).await
    {
        return error(e);
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    clear_cookie(&mut response);
    response
}
fn profile(view: SessionView) -> Response {
    Json(ProfileBody {
        name: view.name,
        preferred_locale: view.locale.map(crate::localization::tag),
    })
    .into_response()
}
pub(crate) fn clear_cookie(response: &mut Response) {
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static(
            "__Host-darkhorse=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0",
        ),
    );
}
fn error(error: AuthError) -> Response {
    let (status, code) = match error {
        AuthError::Denied => (StatusCode::UNAUTHORIZED, "invalid_credentials"),
        AuthError::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable"),
        AuthError::Limited { .. } => (StatusCode::TOO_MANY_REQUESTS, "try_later"),
    };
    let mut response = (status, Json(ErrorBody { error: code })).into_response();
    if let AuthError::Limited { retry_after_ms } = error {
        response.headers_mut().insert(
            header::RETRY_AFTER,
            HeaderValue::from(retry_after_ms.div_ceil(1000).max(1)),
        );
    }
    if error == AuthError::Denied {
        clear_cookie(&mut response);
    }
    response
}

#[cfg(test)]
#[path = "../tests/unit/authentication_http.rs"]
mod tests;
