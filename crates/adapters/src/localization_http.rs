//! Public, nonsecret presentation configuration; no account lookup or database work.
use axum::{
    Json, Router,
    http::{HeaderValue, header},
    routing::get,
};
use darkhorse_domain::localization::Locale;
pub fn router(locale: Locale, origin: url::Url) -> Router {
    let routes = Router::new().route(
        "/api/presentation",
        get(move || async move {
            Json(serde_json::json!({"default_locale":crate::localization::tag(locale)}))
        }),
    );
    crate::authentication_http::protect(routes, origin, 1024, 16).layer(
        tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ),
    )
}
#[cfg(test)]
#[path = "../tests/unit/localization_http.rs"]
mod tests;
