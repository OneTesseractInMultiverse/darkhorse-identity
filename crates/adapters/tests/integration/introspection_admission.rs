use super::{Fixture, SERIAL};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use darkhorse_adapters::{
    http,
    introspection_admission::{Policy, SharedBudgets},
    login_admission::SharedLoginAdmission,
};
use darkhorse_application::shared_limiting::EnforcementAuthority;
use darkhorse_application::{
    authentication::LoginAdmission,
    introspection_admission::{Budgets, Caller},
    signing::WrappedKey,
    tokens::{IdClaims, IdSigner},
};
use darkhorse_domain::{identity::ResourceId, tokens::Error};
use tower::ServiceExt;
use uuid::Uuid;

struct NoSigner;
impl IdSigner for NoSigner {
    async fn sign(&self, _: WrappedKey, _: IdClaims) -> Result<String, Error> {
        panic!("introspection must never sign")
    }
}
fn router(f: &Fixture, global: u32, caller: u32) -> Router {
    with_budgets(
        f,
        SharedBudgets::new(f.limiter(), [7; 32], Policy::new(global, caller).unwrap()),
    )
}
fn with_budgets(f: &Fixture, budgets: impl Budgets + 'static) -> Router {
    let gate = darkhorse_application::introspection_admission::Service {
        store: f.store.clone(),
        budgets,
    };
    http::with_authentication(
        "unused-integration-assets".into(),
        darkhorse_adapters::token_http::router(
            f.store.clone(),
            NoSigner,
            url::Url::parse("https://localhost:8443").unwrap(),
            gate,
        ),
    )
}
async fn resource(f: &Fixture) -> (Uuid, String) {
    let principal = Uuid::new_v4();
    let application = Uuid::new_v4();
    let resource = Uuid::new_v4();
    // Resource secret syntax is obtained from the production entropy adapter.
    use darkhorse_application::resource_servers::Entropy;
    let material = darkhorse_adapters::resource_servers::OsResourceEntropy
        .secret()
        .unwrap();
    sqlx::query(
        "INSERT INTO principals(id,email,first_name,last_name) VALUES($1,$2,'Admission','Test')",
    )
    .bind(principal)
    .bind(format!("{principal}@example.com"))
    .execute(&f.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO applications(id,name,owner_id,active) VALUES($1,'Admission test',$2,true)",
    )
    .bind(application)
    .bind(principal)
    .execute(&f.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO protected_resources(id,application_id,name,audience) VALUES($1,$2,'API',$3)",
    )
    .bind(resource)
    .bind(application)
    .bind(format!("urn:darkhorse:resource:{resource}"))
    .execute(&f.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO resource_introspection(resource_id,application_id) VALUES($1,$2)")
        .bind(resource)
        .bind(application)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO resource_introspection_secrets(id,resource_id,verifier,created_ms) VALUES($1,$2,$3,0)").bind(Uuid::from_u128(material.verifier.id.as_u128())).bind(resource).bind(material.verifier.digest.as_slice()).execute(&f.pool).await.unwrap();
    (resource, material.value)
}
async fn call(
    app: &Router,
    id: Uuid,
    secret: &str,
    token: &str,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/introspect")
                .method("POST")
                .header("host", "localhost:8443")
                .header("content-type", "application/x-www-form-urlencoded")
                .header(
                    "authorization",
                    format!("Basic {}", STANDARD.encode(format!("rs_{id}:{secret}"))),
                )
                .body(Body::from(format!("token={token}")))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["pragma"], "no-cache");
    if status == StatusCode::TOO_MANY_REQUESTS {
        assert!(
            (1..=60).contains(
                &response.headers()["retry-after"]
                    .to_str()
                    .unwrap()
                    .parse::<u32>()
                    .unwrap()
            )
        );
    }
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains(secret));
    (status, serde_json::from_slice(&bytes).unwrap())
}
#[tokio::test]
async fn shared_http_budgets_ignore_forged_ids_isolate_callers_and_preserve_fencing() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    let (id, secret) = resource(&f).await;
    let (healthy, healthy_secret) = resource(&f).await;
    let first = router(&f, 200, 2);
    let second = router(&f, 200, 2);
    let before = f
        .counters
        .status(f.store.read().await.unwrap())
        .await
        .unwrap();
    for _ in 0..20 {
        assert_eq!(
            call(&first, Uuid::new_v4(), &secret, "invalid").await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        f.counters
            .status(f.store.read().await.unwrap())
            .await
            .unwrap(),
        before + 1,
        "random IDs create only the global budget"
    );
    assert_eq!(
        call(&first, id, &healthy_secret, "invalid").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&first, id, &secret, "invalid").await,
        (StatusCode::OK, serde_json::json!({"active":false}))
    );
    assert_eq!(
        call(&second, id, &secret, "dk_invalid").await.0,
        StatusCode::OK,
        "key and OAuth inquiries share a resource budget"
    );
    assert_eq!(
        call(&first, id, &secret, "invalid").await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        call(&second, healthy, &healthy_secret, "invalid").await.0,
        StatusCode::OK
    );
    assert_eq!(
        SharedLoginAdmission::new(f.limiter(), [7; 32])
            .admit("admission-login@example.com")
            .await,
        Ok(())
    );
    // A new process-local pool neither resets global nor authenticated budgets.
    assert_eq!(
        call(&router(&f, 200, 2), id, &secret, "invalid").await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    sqlx::query(
        "UPDATE resource_introspection SET active=false,revision=revision+1 WHERE resource_id=$1",
    )
    .bind(healthy)
    .execute(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        call(&second, healthy, &healthy_secret, "invalid").await.0,
        StatusCode::UNAUTHORIZED
    );
    f.fence().await;
    assert_eq!(
        call(&first, id, &secret, "invalid").await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        call(&router(&f, 200, 2), id, &secret, "invalid").await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}
#[tokio::test]
async fn global_quota_and_policy_mismatch_fail_closed_without_charging_login() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    let app = router(&f, 2, 1);
    let (id, secret) = resource(&f).await;
    for _ in 0..2 {
        assert_eq!(
            call(&app, Uuid::new_v4(), &secret, "invalid").await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        call(&app, id, &secret, "invalid").await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        call(&router(&f, 3, 1), id, &secret, "invalid").await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        call(&router(&f, 2, 2), Uuid::new_v4(), &secret, "invalid")
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE,
        "different caller policy is rejected globally before selecting any new caller key"
    );
    assert_eq!(
        SharedLoginAdmission::new(f.limiter(), [7; 32])
            .admit("quota-login@example.com")
            .await,
        Ok(())
    );
}
#[tokio::test]
async fn caller_budget_is_shared_across_parallel_replica_instances() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    let id = Caller::Resource(ResourceId::from_u128(99).unwrap());
    let a = SharedBudgets::new(f.limiter(), [7; 32], Policy::new(100, 5).unwrap());
    let b = SharedBudgets::new(f.limiter(), [7; 32], Policy::new(100, 5).unwrap());
    let mut admitted = 0;
    for _ in 0..20 {
        let (x, y) = tokio::join!(a.caller(id), b.caller(id));
        admitted += usize::from(x.is_ok()) + usize::from(y.is_ok());
    }
    assert_eq!(admitted, 5);
}

struct RevokeAfterAdmission {
    budgets: SharedBudgets,
    pool: sqlx::PgPool,
}
impl Budgets for RevokeAfterAdmission {
    async fn global(&self) -> Result<(), Error> {
        self.budgets.global().await
    }
    async fn caller(&self, caller: Caller) -> Result<(), Error> {
        self.budgets.caller(caller).await?;
        let Caller::Resource(resource) = caller else {
            panic!("resource fixture only")
        };
        let mut tx = self.pool.begin().await.unwrap();
        sqlx::query("SELECT singleton FROM security_state WHERE singleton FOR UPDATE")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        sqlx::query("UPDATE resource_introspection SET active=false,revision=revision+1 WHERE resource_id=$1").bind(Uuid::from_u128(resource.as_u128())).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        Ok(())
    }
}
#[tokio::test]
async fn final_introspection_reauthenticates_after_the_admission_transaction() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    let (id, secret) = resource(&f).await;
    let app = with_budgets(
        &f,
        RevokeAfterAdmission {
            budgets: SharedBudgets::new(f.limiter(), [7; 32], Policy::new(100, 10).unwrap()),
            pool: f.pool.clone(),
        },
    );
    assert_eq!(
        call(&app, id, &secret, "invalid").await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn queued_global_admission_times_out_and_cancellation_never_runs_later() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&super::variable("DARKHORSE_TEST_DATABASE_URL"))
        .await
        .unwrap();
    let held = pool.acquire().await.unwrap();
    let limiter = darkhorse_adapters::redis_limiter::RedisLimiter::new(
        darkhorse_adapters::postgres::PostgresStore::from_pool(pool.clone()),
        super::settings(false),
    )
    .unwrap();
    let budgets = SharedBudgets::new(limiter, [7; 32], Policy::new(2, 1).unwrap());
    let start = std::time::Instant::now();
    let (a, b) = tokio::join!(budgets.global(), budgets.global());
    assert_eq!(a, Err(Error::Unavailable));
    assert_eq!(b, Err(Error::Unavailable));
    assert!(start.elapsed() < std::time::Duration::from_secs(3));
    drop(held);
    assert_eq!(
        f.counters
            .status(f.store.read().await.unwrap())
            .await
            .unwrap(),
        0
    );
    assert_eq!(budgets.global().await, Ok(()));
    assert_eq!(budgets.global().await, Ok(()));
    assert!(matches!(budgets.global().await, Err(Error::Limited { .. })));
    pool.close().await;
}
