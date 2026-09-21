use crate::{
    authentication_http,
    media::{images::Decoder, objects::Storage},
    profiles_http,
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use darkhorse_application::media::{Service, Store, Target};
use darkhorse_domain::{
    media::{Kind, MAX_BYTES},
    profiles::Error,
};
use serde_json::json;
use std::sync::Arc;
struct Endpoint<S> {
    service: Service<S, Storage, Decoder>,
}
pub fn router<S: Store + 'static>(
    service: Service<S, Storage, Decoder>,
    origin: url::Url,
) -> Router {
    let state = Arc::new(Endpoint { service });
    let reads = Router::new()
        .route("/api/profiles/{target}/picture", get(portrait::<S>))
        .route("/api/admin/branding", get(settings::<S>))
        .route("/api/branding", get(public_settings::<S>))
        .route("/api/branding/{kind}", get(brand::<S>))
        .with_state(state.clone());
    let writes = Router::new()
        .route(
            "/api/profiles/{target}/picture",
            axum::routing::post(upload_portrait::<S>).delete(remove_portrait::<S>),
        )
        .route(
            "/api/admin/branding/{kind}",
            axum::routing::post(upload_brand::<S>).delete(remove_brand::<S>),
        )
        .with_state(state);
    authentication_http::protect(reads, origin.clone(), 0, 16)
        .merge(authentication_http::protect(writes, origin, MAX_BYTES, 4))
        .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
}
fn portrait_target(value: &str) -> Result<Target, Error> {
    crate::profiles_http::input::target(value).map(|principal| Target {
        kind: Kind::Portrait,
        principal,
    })
}
fn brand_target(value: &str) -> Result<Target, Error> {
    Ok(Target {
        kind: match value {
            "logo" => Kind::Logo,
            "background" => Kind::Background,
            _ => return Err(Error::Invalid),
        },
        principal: None,
    })
}
fn one<'a>(h: &'a HeaderMap, key: &str) -> Result<&'a str, Error> {
    let mut values = h.get_all(key).iter();
    let value = values
        .next()
        .ok_or(Error::Invalid)?
        .to_str()
        .map_err(|_| Error::Invalid)?;
    if values.next().is_some() {
        return Err(Error::Invalid);
    }
    Ok(value)
}
fn revision(headers: &HeaderMap) -> Result<u64, Error> {
    crate::admin_directory_http::counter(one(headers, "x-darkhorse-revision")?)
        .map_err(|_| Error::Invalid)
}
fn picture(result: Result<Option<Vec<u8>>, Error>) -> Response {
    match result {
        Ok(Some(bytes)) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (
                    header::CONTENT_DISPOSITION,
                    "inline; filename=\"image.png\"",
                ),
                (
                    header::CONTENT_SECURITY_POLICY,
                    "default-src 'none'; sandbox",
                ),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                (
                    header::HeaderName::from_static("cross-origin-resource-policy"),
                    "same-origin",
                ),
            ],
            bytes,
        )
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => profiles_http::failure(e),
    }
}
async fn portrait<S: Store>(
    State(e): State<Arc<Endpoint<S>>>,
    headers: HeaderMap,
    Path(value): Path<String>,
) -> Response {
    let input =
        profiles_http::actor(&headers).and_then(|a| portrait_target(&value).map(|t| (a, t)));
    match input {
        Ok((actor, target)) => picture(e.service.read(Some(actor), target).await),
        Err(e) => profiles_http::failure(e),
    }
}
async fn brand<S: Store>(State(e): State<Arc<Endpoint<S>>>, Path(value): Path<String>) -> Response {
    match brand_target(&value) {
        Ok(target) => picture(e.service.read(None, target).await),
        Err(e) => profiles_http::failure(e),
    }
}
async fn upload_portrait<S: Store>(
    State(e): State<Arc<Endpoint<S>>>,
    headers: HeaderMap,
    Path(value): Path<String>,
    bytes: Bytes,
) -> Response {
    upload(&e, &headers, portrait_target(&value), bytes).await
}
async fn upload_brand<S: Store>(
    State(e): State<Arc<Endpoint<S>>>,
    headers: HeaderMap,
    Path(value): Path<String>,
    bytes: Bytes,
) -> Response {
    upload(&e, &headers, brand_target(&value), bytes).await
}
async fn upload<S: Store>(
    e: &Endpoint<S>,
    headers: &HeaderMap,
    target: Result<Target, Error>,
    bytes: Bytes,
) -> Response {
    let parsed = profiles_http::actor(headers).and_then(|actor| {
        Ok((
            actor,
            target?,
            revision(headers)?,
            one(headers, "content-type")?.to_owned(),
        ))
    });
    let (actor, target, revision, mime) = match parsed {
        Ok(v) => v,
        Err(e) => return profiles_http::failure(e),
    };
    if !e.service.objects.enabled() {
        return profiles_http::failure(Error::Unavailable);
    }
    changed(
        e.service
            .upload(actor, target, revision, mime, bytes.to_vec())
            .await,
    )
}
async fn remove_portrait<S: Store>(
    State(e): State<Arc<Endpoint<S>>>,
    headers: HeaderMap,
    Path(value): Path<String>,
) -> Response {
    remove(&e, &headers, portrait_target(&value)).await
}
async fn remove_brand<S: Store>(
    State(e): State<Arc<Endpoint<S>>>,
    headers: HeaderMap,
    Path(value): Path<String>,
) -> Response {
    remove(&e, &headers, brand_target(&value)).await
}
async fn remove<S: Store>(
    e: &Endpoint<S>,
    headers: &HeaderMap,
    target: Result<Target, Error>,
) -> Response {
    match profiles_http::actor(headers).and_then(|actor| Ok((actor, target?, revision(headers)?))) {
        Ok((actor, target, revision)) => {
            changed(e.service.store.remove(actor, target, revision).await)
        }
        Err(e) => profiles_http::failure(e),
    }
}
fn changed(result: Result<u64, Error>) -> Response {
    match result {
        Ok(revision) => Json(json!({"revision":revision.to_string()})).into_response(),
        Err(e) => profiles_http::failure(e),
    }
}
async fn settings<S: Store>(State(e): State<Arc<Endpoint<S>>>, headers: HeaderMap) -> Response {
    let actor = match profiles_http::actor(&headers) {
        Ok(a) => a,
        Err(e) => return profiles_http::failure(e),
    };
    match e.service.store.branding(actor).await {
        Ok(branding) => Json(project_settings(branding, &e.service.objects)).into_response(),
        Err(error) => profiles_http::failure(error),
    }
}
fn project_settings(
    branding: darkhorse_application::media::Branding,
    storage: &Storage,
) -> serde_json::Value {
    json!({
        "revision": branding.revision.to_string(),
        "logo": branding.logo,
        "background": branding.background,
        "storage_enabled": storage.enabled(),
        "bucket": storage.bucket
    })
}
async fn public_settings<S: Store>(State(e): State<Arc<Endpoint<S>>>) -> Response {
    let logo = e
        .service
        .store
        .asset(
            None,
            Target {
                kind: Kind::Logo,
                principal: None,
            },
        )
        .await;
    let background = e
        .service
        .store
        .asset(
            None,
            Target {
                kind: Kind::Background,
                principal: None,
            },
        )
        .await;
    match (logo, background) {
        (Ok(l), Ok(b)) => {
            Json(json!({"logo":l.is_some(),"background":b.is_some()})).into_response()
        }
        _ => profiles_http::failure(Error::Unavailable),
    }
}
#[cfg(test)]
#[path = "../../tests/unit/media_http.rs"]
mod tests;
