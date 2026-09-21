//! An instantaneous primary-database probe, never an authorization decision.
use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use std::{future::Future, sync::Arc, time::Duration};
use tokio::sync::Semaphore;

pub trait Readiness: Send + Sync {
    fn ready(&self) -> impl Future<Output = bool> + Send;
}
struct Boundary<P> {
    probe: P,
    slots: Semaphore,
}
pub fn router<P: Readiness + 'static>(probe: P) -> Router {
    Router::new()
        .route("/health/ready", get(check::<P>))
        .with_state(Arc::new(Boundary {
            probe,
            slots: Semaphore::new(1),
        }))
}
async fn check<P: Readiness>(State(state): State<Arc<Boundary<P>>>) -> Response {
    let Ok(_slot) = state.slots.try_acquire() else {
        return response(false);
    };
    let ready = tokio::time::timeout(Duration::from_secs(1), state.probe.ready())
        .await
        .unwrap_or(false);
    response(ready)
}
fn response(ready: bool) -> Response {
    let (code, status) = if ready {
        (StatusCode::OK, "ready")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "unavailable")
    };
    (
        code,
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::json!({"status":status})),
    )
        .into_response()
}
pub struct Disabled;
impl Readiness for Disabled {
    async fn ready(&self) -> bool {
        false
    }
}
#[cfg(test)]
#[path = "../tests/unit/readiness.rs"]
mod tests;
