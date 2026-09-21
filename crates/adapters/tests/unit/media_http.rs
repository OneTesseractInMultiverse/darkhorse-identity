use super::*;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use darkhorse_application::media::{Asset, Branding, Prepared, Ticket};
use darkhorse_domain::identity::AssetId;
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::ServiceExt;
struct Fake {
    calls: Arc<AtomicUsize>,
    error: Option<Error>,
}
impl Store for Fake {
    async fn reserve(&self, _: [u8; 32], _: Target, _: u64) -> Result<Ticket, Error> {
        panic!("disabled storage must never reserve an upload")
    }
    async fn attach(&self, _: [u8; 32], _: Ticket, _: u64, _: &Prepared) -> Result<u64, Error> {
        panic!("disabled storage must never publish an upload")
    }
    async fn remove(&self, _: [u8; 32], _: Target, revision: u64) -> Result<u64, Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.error.map_or(Ok(revision + 1), Err)
    }
    async fn asset(&self, _: Option<[u8; 32]>, _: Target) -> Result<Option<Asset>, Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.error.map_or(Ok(None), Err)
    }
    async fn branding(&self, _: [u8; 32]) -> Result<Branding, Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.error.map_or(
            Ok(Branding {
                revision: 9,
                logo: false,
                background: false,
            }),
            Err,
        )
    }
    async fn garbage(&self) -> Result<Vec<AssetId>, Error> {
        panic!("HTTP cannot collect objects")
    }
    async fn cleaned(&self, _: AssetId) -> Result<(), Error> {
        panic!("HTTP cannot collect objects")
    }
}
fn app(error: Option<Error>) -> (Router, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    (
        router(
            Service {
                store: Fake {
                    calls: calls.clone(),
                    error,
                },
                objects: Storage::default(),
                images: Decoder::default(),
            },
            url::Url::parse("https://identity.example").unwrap(),
        ),
        calls,
    )
}
fn request(method: &str, path: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("host", "identity.example")
        .header("origin", "https://identity.example")
        .header("x-darkhorse-csrf", "1")
        .header("x-darkhorse-revision", "9")
        .header("content-type", "image/png")
        .header("cookie", format!("__Host-darkhorse={}", "ab".repeat(32)))
        .body(Body::empty())
        .unwrap()
}
#[tokio::test]
async fn malformed_image_writes_never_reach_authority_or_object_ports() {
    let (app, calls) = app(None);
    for path in ["/api/profiles/me/picture", "/api/admin/branding/logo"] {
        for (header, value, status) in [
            ("cookie", None, 401),
            ("origin", Some("https://foreign.example"), 403),
            ("x-darkhorse-csrf", None, 403),
            ("x-darkhorse-revision", Some("01"), 400),
        ] {
            let mut r = request("DELETE", path);
            r.headers_mut().remove(header);
            if let Some(value) = value {
                r.headers_mut().insert(header, value.parse().unwrap());
            }
            assert_eq!(app.clone().oneshot(r).await.unwrap().status(), status);
        }
        let mut r = request("DELETE", path);
        r.headers_mut()
            .append("x-darkhorse-revision", "10".parse().unwrap());
        assert_eq!(app.clone().oneshot(r).await.unwrap().status(), 400);
        let mut r = request("POST", path);
        *r.body_mut() = Body::from(vec![0; MAX_BYTES + 1]);
        assert_eq!(app.clone().oneshot(r).await.unwrap().status(), 413);
        assert_eq!(
            app.clone()
                .oneshot(request("POST", path))
                .await
                .unwrap()
                .status(),
            503
        );
    }
    for path in ["/api/profiles/bad/picture", "/api/admin/branding/external"] {
        assert_eq!(
            app.clone()
                .oneshot(request("DELETE", path))
                .await
                .unwrap()
                .status(),
            400
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn public_branding_is_minimized_and_disabled_storage_still_allows_reference_removal() {
    let (app, _) = app(None);
    for (method, path, expected) in [
        (
            "GET",
            "/api/branding",
            json!({"logo":false,"background":false}),
        ),
        (
            "GET",
            "/api/admin/branding",
            json!({"revision":"9","logo":false,"background":false,"storage_enabled":false,"bucket":null}),
        ),
        (
            "DELETE",
            "/api/admin/branding/background",
            json!({"revision":"10"}),
        ),
        (
            "DELETE",
            "/api/profiles/me/picture",
            json!({"revision":"10"}),
        ),
    ] {
        let mut r = request(method, path);
        if path == "/api/branding" {
            r.headers_mut().remove("cookie");
        }
        let r = app.clone().oneshot(r).await.unwrap();
        assert_eq!(r.status(), 200);
        assert_eq!(r.headers()["cache-control"], "no-store");
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(r.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(body, expected);
    }
    for path in ["/api/branding/logo", "/api/profiles/me/picture"] {
        assert_eq!(
            app.clone()
                .oneshot(request("GET", path))
                .await
                .unwrap()
                .status(),
            404
        );
    }
}
#[tokio::test]
async fn storage_failures_are_not_public_metadata_or_successful_removals() {
    for path in [
        "/api/branding",
        "/api/branding/logo",
        "/api/admin/branding",
        "/api/profiles/me/picture",
    ] {
        let (app, _) = app(Some(Error::Unavailable));
        assert_eq!(
            app.oneshot(request("GET", path)).await.unwrap().status(),
            503
        );
    }
    for path in ["/api/profiles/me/picture", "/api/admin/branding/logo"] {
        let (app, _) = app(Some(Error::Conflict));
        assert_eq!(
            app.oneshot(request("DELETE", path)).await.unwrap().status(),
            409
        );
    }
}
#[test]
fn image_headers_prevent_active_content_and_cross_origin_embedding() {
    let r = picture(Ok(Some(vec![1, 2, 3])));
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["content-type"], "image/png");
    assert_eq!(
        r.headers()["content-security-policy"],
        "default-src 'none'; sandbox"
    );
    assert_eq!(r.headers()["x-content-type-options"], "nosniff");
    assert_eq!(r.headers()["cross-origin-resource-policy"], "same-origin");
}
