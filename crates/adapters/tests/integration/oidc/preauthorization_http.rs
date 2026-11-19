use super::*;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request as HttpRequest, header},
    response::Response,
};
use darkhorse_adapters::provider_http::{self, ContinuationKey};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

async fn call(
    router: &Router,
    path: &str,
    cookies: &str,
    body: Option<serde_json::Value>,
) -> Response {
    let mut request = HttpRequest::builder()
        .uri(path)
        .header("host", "issuer.example");
    if !cookies.is_empty() {
        request = request.header("cookie", cookies);
    }
    if body.is_some() {
        request = request
            .method("POST")
            .header("origin", "https://issuer.example")
            .header("x-darkhorse-csrf", "1")
            .header("content-type", "application/json");
    }
    router
        .clone()
        .oneshot(
            request
                .body(Body::from(body.map(|v| v.to_string()).unwrap_or_default()))
                .unwrap(),
        )
        .await
        .unwrap()
}
async fn json(response: Response) -> serde_json::Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 32_768).await.unwrap()).unwrap()
}
fn flow(response: Response) -> (String, String) {
    assert_eq!(response.status(), 303);
    let reference = response.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .strip_prefix("/authorization?request=")
        .unwrap()
        .to_owned();
    let cookie = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    (reference, cookie)
}
#[tokio::test]
async fn anonymous_http_flood_cannot_prevent_a_fresh_browser_sign_in_and_code_issue() {
    let db = fixture().await;
    let router = provider_http::router(
        db.store.clone(),
        url::Url::parse("https://issuer.example").unwrap(),
        ContinuationKey::derive(&[9; 32]).unwrap(),
    );
    let path = format!(
        "/authorize?client_id=00000000-0000-0000-0000-000000000020&redirect_uri=https%3A%2F%2Fclient.example%2Fcallback%3Ffixed%3D1&response_type=code&scope=openid&state=retained-state&nonce=retained-nonce&code_challenge={}&code_challenge_method=S256",
        "A".repeat(43)
    );
    for n in 0..110 {
        let cookies = if n % 2 == 0 {
            String::new()
        } else {
            format!("__Host-darkhorse={}", "99".repeat(32))
        };
        assert_eq!(call(&router, &path, &cookies, None).await.status(), 303);
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM authorization_requests")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    let (reference, proof) = flow(call(&router, &path, "", None).await);
    let (other, _) = flow(call(&router, &path, "", None).await);
    let inspect = format!("/api/authorization?request={reference}");
    assert_eq!(call(&router, &inspect, "", None).await.status(), 400);
    let substituted = format!(
        "/api/authorization?request={}.{}",
        &reference[..64],
        other.split_once('.').unwrap().1
    );
    assert_eq!(
        call(&router, &substituted, &proof, None).await.status(),
        400
    );
    let waiting = json(call(&router, &inspect, &proof, None).await).await;
    assert_eq!(waiting["status"], "login");
    assert_eq!(waiting["request_id"], &reference[..64]);
    let raw = "ab".repeat(32);
    let candidate = db
        .store
        .candidate("one@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store
        .establish(&candidate, Sha256::digest(raw.as_bytes()).into(), None)
        .await
        .unwrap();
    let cookies = format!("{proof}; __Host-darkhorse={raw}");
    let view = json(call(&router, &inspect, &cookies, None).await).await;
    assert_eq!(view["status"], "consent");
    let selector = &reference[..64];
    let outcome = call(
        &router,
        &format!("/api/authorization/decision?request={reference}"),
        &cookies,
        Some(serde_json::json!({"request_id":selector,"decision":"approve"})),
    )
    .await;
    assert_eq!(outcome.status(), 200);
    assert!(
        outcome.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .starts_with(&format!("__Host-darkhorse-authorization-{selector}=;"))
    );
    let value = json(outcome).await;
    let redirect = url::Url::parse(value["redirect"].as_str().unwrap()).unwrap();
    let fields: std::collections::BTreeMap<_, _> = redirect.query_pairs().collect();
    assert_eq!(fields.get("state").unwrap(), "retained-state");
    assert_eq!(fields.get("fixed").unwrap(), "1");
    assert_eq!(fields.get("iss").unwrap(), "https://issuer.example");
    assert!(fields.contains_key("code"));
    assert_eq!(call(&router, &inspect, &cookies, None).await.status(), 400);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM authorization_requests")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    let retained: (String, Vec<u8>) =
        sqlx::query_as("SELECT nonce,challenge FROM authorization_requests")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(retained, ("retained-nonce".into(), vec![0; 32]));
    db.store.close().await;
}
