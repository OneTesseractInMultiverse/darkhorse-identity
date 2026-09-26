//! Public, nonsecret presentation configuration; no account lookup or database work.
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderValue, header},
    routing::get,
};
use darkhorse_domain::localization::Locale;
#[derive(serde::Serialize)]
struct Presentation {
    default_locale: &'static str,
}
pub fn router(locale: Locale, origin: url::Url) -> Router {
    let routes = Router::new()
        .route("/api/presentation", get(presentation))
        .with_state(locale);
    crate::authentication_http::protect(routes, origin, 1024, 16).layer(
        tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ),
    )
}
async fn presentation(State(locale): State<Locale>) -> Json<Presentation> {
    Json(Presentation {
        default_locale: crate::localization::tag(locale),
    })
}
#[cfg(test)]
#[path = "../tests/unit/localization_http.rs"]
mod tests;
