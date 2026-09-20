use axum::{
    Json, Router,
    http::{HeaderValue, StatusCode, header},
    routing::get,
};
use serde::Serialize;
use std::path::PathBuf;
use tower_http::{
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
};

#[derive(Serialize)]
struct Liveness {
    status: &'static str,
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
}

/// Only explicitly registered UI paths serve HTML. Reserved protocol paths
/// must retain their own errors as the provider grows.
pub fn router(static_dir: PathBuf) -> Router {
    with_authentication(static_dir, Router::new())
}

pub fn with_authentication(static_dir: PathBuf, authentication: Router) -> Router {
    Router::new()
        .merge(authentication)
        .route("/health/live", get(liveness))
        .route_service("/", ServeFile::new(static_dir.join("index.html")))
        .route_service(
            "/console/users",
            ServeFile::new(static_dir.join("console/users.html")),
        )
        .route_service(
            "/authorization",
            ServeFile::new(static_dir.join("authorization.html")),
        )
        .route_service(
            "/security/sessions",
            ServeFile::new(static_dir.join("security/sessions.html")),
        )
        .route_service(
            "/invitation",
            ServeFile::new(static_dir.join("invitation.html")),
        )
        .route_service(
            "/security/email",
            ServeFile::new(static_dir.join("security/email.html")),
        )
        .nest_service("/_app", ServeDir::new(static_dir.join("_app")))
        .fallback(not_found)
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("frame-ancestors 'none'; base-uri 'none'; form-action 'self'"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
}

async fn liveness() -> Json<Liveness> {
    Json(Liveness { status: "ok" })
}

async fn not_found() -> (StatusCode, Json<ErrorBody>) {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorBody { error: "not_found" }),
    )
}

#[cfg(test)]
#[path = "../tests/unit/http.rs"]
mod tests;
