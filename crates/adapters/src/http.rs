use axum::{
    Json, Router,
    extract::Request,
    http::{HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::Response,
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
            "/console/settings",
            ServeFile::new(static_dir.join("console/settings.html")),
        )
        .route_service(
            "/account/profile",
            ServeFile::new(static_dir.join("account/profile.html")),
        )
        .route_service(
            "/console/profile",
            ServeFile::new(static_dir.join("console/profile.html")),
        )
        .route_service(
            "/console/users",
            ServeFile::new(static_dir.join("console/users.html")),
        )
        .route_service(
            "/console/applications",
            ServeFile::new(static_dir.join("console/applications.html")),
        )
        .route_service(
            "/console/clients",
            ServeFile::new(static_dir.join("console/clients.html")),
        )
        .route_service(
            "/console/resources",
            ServeFile::new(static_dir.join("console/resources.html")),
        )
        .route_service(
            "/console/scopes",
            ServeFile::new(static_dir.join("console/scopes.html")),
        )
        .route_service(
            "/console/roles",
            ServeFile::new(static_dir.join("console/roles.html")),
        )
        .route_service(
            "/console/capabilities",
            ServeFile::new(static_dir.join("console/capabilities.html")),
        )
        .route_service(
            "/console/api-docs",
            ServeFile::new(static_dir.join("console/api-docs.html")),
        )
        .route_service(
            "/console/policies",
            ServeFile::new(static_dir.join("console/policies.html")),
        )
        .route_service(
            "/reference/openapi-v1.json",
            ServeFile::new(static_dir.join("reference/openapi-v1.json")),
        )
        .route_service(
            "/reference/route-classification-v1.json",
            ServeFile::new(static_dir.join("reference/route-classification-v1.json")),
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
            "/security/keys",
            ServeFile::new(static_dir.join("security/keys.html")),
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
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("frame-ancestors 'none'; base-uri 'none'; form-action 'self'"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(middleware::from_fn(cache_static_response))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
}

async fn cache_static_response(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_owned();
    let mut response = next.run(request).await;
    let value = cache_control_for_response(&path, response.status());
    response.headers_mut().insert(header::CACHE_CONTROL, value);
    response
}

fn cache_control_for_response(path: &str, status: StatusCode) -> HeaderValue {
    if path.starts_with("/_app/immutable/") && status.is_success() {
        HeaderValue::from_static("public, max-age=31536000, immutable")
    } else {
        HeaderValue::from_static("no-store")
    }
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
