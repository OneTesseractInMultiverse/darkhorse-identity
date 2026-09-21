use super::*;
use axum::{body::Body, http::Request};
use tower::ServiceExt;
struct Probe(bool);
impl Readiness for Probe {
    async fn ready(&self) -> bool {
        self.0
    }
}
#[tokio::test]
async fn readiness_is_bounded_redacted_and_never_replaces_liveness() {
    for (available, status) in [(true, 200), (false, 503)] {
        let response = router(Probe(available))
            .oneshot(
                Request::builder()
                    .uri("/health/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body = axum::body::to_bytes(response.into_body(), 128)
            .await
            .unwrap();
        assert_eq!(
            body.as_ref(),
            if available {
                b"{\"status\":\"ready\"}".as_slice()
            } else {
                b"{\"status\":\"unavailable\"}".as_slice()
            }
        );
    }
}

struct Pending;
impl Readiness for Pending {
    async fn ready(&self) -> bool {
        std::future::pending().await
    }
}
#[tokio::test]
async fn deadline_and_unconfigured_service_are_unavailable() {
    for app in [router(Pending), router(Disabled)] {
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 503);
    }
}
#[tokio::test]
async fn concurrent_probe_does_not_queue_behind_a_slow_dependency() {
    let state = Arc::new(Boundary {
        probe: Probe(true),
        slots: Semaphore::new(1),
    });
    let held = state.slots.acquire().await.unwrap();
    assert_eq!(check(State(state.clone())).await.status(), 503);
    drop(held);
    assert_eq!(check(State(state)).await.status(), 200);
}
