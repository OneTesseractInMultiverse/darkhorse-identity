use darkhorse_adapters::{
    postgres::PostgresStore,
    redis_configuration,
    redis_limiter::{RedisCounters, RedisLimiter},
};
use darkhorse_application::{
    limiting::{Admission, AttemptLimiter, LimiterUnavailable},
    shared_limiting::{self, CounterStore, EnforcementAuthority, RecoveryAuthority},
};
use darkhorse_domain::{
    limiter_recovery::Enforcement,
    limiting::{Attempt, Budget, BudgetRule},
};
use redis::aio::MultiplexedConnection;
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
fn variable(name: &str) -> String {
    std::env::var(name).expect("use make test-redis for disposable infrastructure")
}
fn settings(operator: bool) -> redis_configuration::RedisSettings {
    settings_with(operator, vec![])
}
fn settings_with(
    operator: bool,
    overrides: Vec<(&str, String)>,
) -> redis_configuration::RedisSettings {
    let mut values = vec![
        (
            "DARKHORSE_REDIS_CACHE_URL",
            variable("DARKHORSE_REDIS_CACHE_URL"),
        ),
        (
            "DARKHORSE_REDIS_LIMITER_URL",
            variable(if operator {
                "DARKHORSE_REDIS_LIMITER_ADMIN_URL"
            } else {
                "DARKHORSE_REDIS_LIMITER_URL"
            }),
        ),
        ("DARKHORSE_REDIS_INSECURE", "true".into()),
        ("DARKHORSE_REDIS_LIMITER_CONNECTIONS", "16".into()),
        ("DARKHORSE_REDIS_TIMEOUT_MS", "1000".into()),
    ];
    values.extend(overrides);
    redis_configuration::load(envbind::MapEnvironment::from_pairs(values)).unwrap()
}

fn attempt(key: u8, limit: u32, window: u32) -> Attempt {
    Attempt::new(vec![Budget {
        key: [key; 32],
        rule: BudgetRule::new(limit, window).unwrap(),
    }])
    .unwrap()
}
struct Fixture {
    store: PostgresStore,
    pool: PgPool,
    counters: RedisCounters,
    operator: RedisCounters,
    admin: MultiplexedConnection,
}
impl Fixture {
    async fn new() -> Self {
        let pool = PgPool::connect(&variable("DARKHORSE_TEST_DATABASE_URL"))
            .await
            .unwrap();
        let store = PostgresStore::from_pool(pool.clone());
        store.migrate().await.unwrap();
        let admin = redis::Client::open(variable("DARKHORSE_REDIS_LIMITER_ADMIN_URL"))
            .unwrap()
            .get_multiplexed_async_connection()
            .await
            .unwrap();
        Self {
            store,
            pool,
            admin,
            counters: RedisCounters::new(settings(false)).unwrap(),
            operator: RedisCounters::new(settings(true)).unwrap(),
        }
    }
    async fn fence(&self) -> Enforcement {
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).unwrap();
        self.store.fence(nonce).await.unwrap()
    }
    async fn expire_wait(&self) {
        // Only this disposable database's owner advances the test fixture clock.
        // There is no production wait override or environment switch.
        sqlx::query("ALTER TABLE limiter_authority DISABLE TRIGGER limiter_authority_transition")
            .execute(&self.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE limiter_authority SET not_before_ms=0")
            .execute(&self.pool)
            .await
            .unwrap();
        sqlx::query("ALTER TABLE limiter_authority ENABLE TRIGGER limiter_authority_transition")
            .execute(&self.pool)
            .await
            .unwrap();
    }
    async fn activate(&self) {
        self.fence().await;
        self.expire_wait().await;
        let state = self.store.read().await.unwrap();
        let id = self.operator.initialize(state).await.unwrap();
        self.store.activate(state.generation, id).await.unwrap();
    }
    fn limiter(&self) -> RedisLimiter {
        RedisLimiter::new(self.store.clone(), settings(false)).unwrap()
    }
}

#[tokio::test]
async fn recovery_is_durable_waited_and_same_generation_initialization_never_resets() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    let cooling = f.fence().await;
    assert!(cooling.not_before_ms - cooling.now_ms >= 903_000);
    assert!(f.operator.initialize(cooling).await.is_err());
    assert_eq!(
        f.limiter().consume(&attempt(1, 1, 1000)).await,
        Err(LimiterUnavailable)
    );
    f.expire_wait().await;
    let ready = f.store.read().await.unwrap();
    assert!(
        f.counters.initialize(ready).await.is_err(),
        "runtime ACL cannot initialize"
    );
    let id = f.operator.initialize(ready).await.unwrap();
    f.store.activate(ready.generation, id).await.unwrap();
    let limiter = f.limiter();
    let a = attempt(1, 1, 1000);
    assert_eq!(limiter.consume(&a).await, Ok(Admission::Allowed));
    f.operator.initialize(ready).await.unwrap();
    assert!(matches!(
        limiter.consume(&a).await,
        Ok(Admission::Limited { .. })
    ));
    assert!(f.store.activate(ready.generation, id).await.is_err());
    let cooling = f.fence().await;
    assert_eq!(limiter.consume(&a).await, Err(LimiterUnavailable));
    assert_eq!(f.limiter().consume(&a).await, Err(LimiterUnavailable));
    assert!(f.store.activate(ready.generation, id).await.is_err());
    assert_ne!(cooling.generation, ready.generation);
    let audits: i64 = sqlx::query_scalar("SELECT count(*) FROM limiter_audit")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert!(audits >= 3);
    assert!(
        sqlx::query("DELETE FROM limiter_authority")
            .execute(&f.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE limiter_audit SET event='limiter.fenced'")
            .execute(&f.pool)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn two_replicas_charge_multiple_budgets_atomically_under_contention() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    let left = Arc::new(f.limiter());
    let right = Arc::new(f.limiter());
    let a = Attempt::new(vec![
        Budget {
            key: [2; 32],
            rule: BudgetRule::new(3, 10_000).unwrap(),
        },
        Budget {
            key: [3; 32],
            rule: BudgetRule::new(5, 10_000).unwrap(),
        },
    ])
    .unwrap();
    let mut tasks = Vec::new();
    for i in 0..20 {
        let l = if i % 2 == 0 {
            left.clone()
        } else {
            right.clone()
        };
        let a = a.clone();
        tasks.push(tokio::spawn(async move { l.consume(&a).await }));
    }
    let mut allowed = 0;
    for t in tasks {
        if t.await.unwrap() == Ok(Admission::Allowed) {
            allowed += 1;
        }
    }
    assert_eq!(allowed, 3);
    for _ in 0..3 {
        assert!(matches!(
            left.consume(&a).await,
            Ok(Admission::Limited { .. })
        ));
    }
    let other = attempt(3, 5, 10_000);
    assert_eq!(right.consume(&other).await, Ok(Admission::Allowed));
    assert_eq!(left.consume(&other).await, Ok(Admission::Allowed));
    assert!(matches!(
        right.consume(&other).await,
        Ok(Admission::Limited { .. })
    ));
}

#[tokio::test]
async fn expiry_script_reload_and_policy_changes_preserve_budget_boundaries() {
    let _serial = SERIAL.lock().await;
    let mut f = Fixture::new().await;
    f.activate().await;
    let limiter = f.limiter();
    let a = attempt(4, 1, 1000);
    assert_eq!(limiter.consume(&a).await, Ok(Admission::Allowed));
    redis::cmd("SCRIPT")
        .arg("FLUSH")
        .query_async::<()>(&mut f.admin)
        .await
        .unwrap();
    assert!(matches!(
        limiter.consume(&a).await,
        Ok(Admission::Limited { .. })
    ));
    assert_eq!(
        limiter.consume(&attempt(4, 2, 1000)).await,
        Err(LimiterUnavailable)
    );
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(limiter.consume(&a).await, Ok(Admission::Allowed));
    let state = f.store.read().await.unwrap();
    let skewed = Enforcement {
        now_ms: state.now_ms - 5000,
        ..state
    };
    assert!(f.counters.snapshot(skewed, &a).await.is_err());
}

#[tokio::test]
async fn missing_counter_or_generation_and_new_processes_never_create_allowance() {
    let _serial = SERIAL.lock().await;
    let mut f = Fixture::new().await;
    f.activate().await;
    let limiter = f.limiter();
    let a = attempt(5, 1, 10_000);
    assert_eq!(limiter.consume(&a).await, Ok(Admission::Allowed));
    redis::cmd("HDEL")
        .arg("darkhorse:limiter:v1")
        .arg(format!("b:{}", "05".repeat(32)))
        .query_async::<i64>(&mut f.admin)
        .await
        .unwrap();
    assert_eq!(limiter.consume(&a).await, Err(LimiterUnavailable));
    assert_eq!(f.limiter().consume(&a).await, Err(LimiterUnavailable));
    redis::cmd("DEL")
        .arg("darkhorse:limiter:v1")
        .query_async::<i64>(&mut f.admin)
        .await
        .unwrap();
    assert_eq!(limiter.consume(&a).await, Err(LimiterUnavailable));
    assert_eq!(f.limiter().consume(&a).await, Err(LimiterUnavailable));
    f.fence().await;
    assert!(
        f.operator
            .initialize(f.store.read().await.unwrap())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn expired_records_are_collected_but_live_cardinality_exhaustion_rejects_new_keys() {
    let _serial = SERIAL.lock().await;
    let mut f = Fixture::new().await;
    f.activate().await;
    let now = f.store.read().await.unwrap().now_ms;
    for expired in [false, true] {
        let start = if expired { now - 2000 } else { now };
        let window = if expired { 1000 } else { 900_000 };
        let mut fill = redis::cmd("HSET");
        fill.arg("darkhorse:limiter:v1").arg("_count").arg(16384);
        for index in 0..16384 {
            fill.arg(format!("b:{index:064x}"))
                .arg(format!("1,{window},1,{start},{start}"));
        }
        fill.query_async::<i64>(&mut f.admin).await.unwrap();
        let result = f.limiter().consume(&attempt(6, 1, 1000)).await;
        if expired {
            assert_eq!(result, Ok(Admission::Allowed));
        } else {
            assert_eq!(result, Err(LimiterUnavailable));
        }
        let count: i64 = redis::cmd("HLEN")
            .arg("darkhorse:limiter:v1")
            .query_async(&mut f.admin)
            .await
            .unwrap();
        assert!(count <= 16389);
    }
}

#[tokio::test]
async fn limiter_memory_acl_partition_restart_and_promotion_fail_closed() {
    let _serial = SERIAL.lock().await;
    let mut f = Fixture::new().await;
    f.activate().await;
    let limiter = f.limiter();
    let a = attempt(7, 1, 900_000);
    assert_eq!(limiter.consume(&a).await, Ok(Admission::Allowed));
    redis::cmd("CONFIG")
        .arg("SET")
        .arg("maxmemory")
        .arg(1)
        .query_async::<()>(&mut f.admin)
        .await
        .unwrap();
    assert_eq!(limiter.consume(&a).await, Err(LimiterUnavailable));
    redis::cmd("CONFIG")
        .arg("SET")
        .arg("maxmemory")
        .arg(67108864)
        .query_async::<()>(&mut f.admin)
        .await
        .unwrap();
    redis::cmd("ACL")
        .arg("SETUSER")
        .arg("darkhorse-limiter")
        .arg("-evalsha")
        .query_async::<()>(&mut f.admin)
        .await
        .unwrap();
    assert_eq!(limiter.consume(&a).await, Err(LimiterUnavailable));
    redis::cmd("ACL")
        .arg("SETUSER")
        .arg("darkhorse-limiter")
        .arg("+evalsha")
        .query_async::<()>(&mut f.admin)
        .await
        .unwrap();
    let container = variable("DARKHORSE_TEST_REDIS_LIMITER_CONTAINER");
    assert!(container.starts_with("darkhorse-redis-test-"));
    let docker = |op: &str| {
        assert!(
            std::process::Command::new("docker")
                .args([op, container.as_str()])
                .stdout(std::process::Stdio::null())
                .status()
                .unwrap()
                .success()
        );
    };
    docker("pause");
    let result = tokio::time::timeout(Duration::from_secs(2), limiter.consume(&a)).await;
    docker("unpause");
    assert_eq!(result.unwrap(), Err(LimiterUnavailable));
    assert!(matches!(
        limiter.consume(&a).await,
        Ok(Admission::Limited { .. })
    ));
    let identity = f.operator.identify().await.unwrap();
    redis::cmd("REPLICAOF")
        .arg("127.0.0.1")
        .arg(1)
        .query_async::<()>(&mut f.admin)
        .await
        .unwrap();
    assert_eq!(limiter.consume(&a).await, Err(LimiterUnavailable));
    redis::cmd("REPLICAOF")
        .arg("NO")
        .arg("ONE")
        .query_async::<()>(&mut f.admin)
        .await
        .unwrap();
    let promoted = f.operator.identify().await.unwrap();
    assert_eq!(promoted.run, identity.run);
    assert_ne!(promoted.replication, identity.replication);
    assert_eq!(limiter.consume(&a).await, Err(LimiterUnavailable));
    f.activate().await;
    assert_eq!(limiter.consume(&a).await, Ok(Admission::Allowed));
    docker("restart");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(limiter.consume(&a).await, Err(LimiterUnavailable));
    assert_eq!(f.limiter().consume(&a).await, Err(LimiterUnavailable));
}

struct LostReply<'a> {
    inner: &'a RedisCounters,
    calls: std::sync::atomic::AtomicUsize,
}
impl CounterStore for LostReply<'_> {
    async fn snapshot(
        &self,
        s: Enforcement,
        a: &Attempt,
    ) -> Result<shared_limiting::Snapshot, LimiterUnavailable> {
        self.inner.snapshot(s, a).await
    }
    async fn apply(
        &self,
        s: Enforcement,
        a: &Attempt,
        snapshot: &shared_limiting::Snapshot,
        c: &[darkhorse_domain::limiting::Counter],
        now: u64,
    ) -> Result<shared_limiting::Commit, LimiterUnavailable> {
        self.calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(
            self.inner.apply(s, a, snapshot, c, now).await?,
            shared_limiting::Commit::Applied
        );
        Err(LimiterUnavailable)
    }
    async fn prune(&self, s: Enforcement) -> Result<(), LimiterUnavailable> {
        self.inner.prune(s).await
    }
}
#[tokio::test]
async fn loss_of_a_committed_reply_is_not_retried_and_still_consumes_the_budget() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    let lost = LostReply {
        inner: &f.counters,
        calls: std::sync::atomic::AtomicUsize::new(0),
    };
    let a = attempt(8, 2, 10_000);
    assert_eq!(
        shared_limiting::consume(&f.store, &lost, &a).await,
        Err(LimiterUnavailable)
    );
    assert_eq!(lost.calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(f.limiter().consume(&a).await, Ok(Admission::Allowed));
    assert!(matches!(
        f.limiter().consume(&a).await,
        Ok(Admission::Limited { .. })
    ));
}

#[tokio::test]
async fn tcp_disconnect_or_timeout_after_committed_response_never_retries_the_write() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    for (index, port) in [
        "DARKHORSE_TEST_REDIS_DROP_PORT",
        "DARKHORSE_TEST_REDIS_TIMEOUT_PORT",
    ]
    .into_iter()
    .enumerate()
    {
        let mut url = url::Url::parse(&variable("DARKHORSE_REDIS_LIMITER_URL")).unwrap();
        url.set_port(Some(variable(port).parse().unwrap())).unwrap();
        let broken = RedisLimiter::new(
            f.store.clone(),
            settings_with(
                false,
                vec![("DARKHORSE_REDIS_LIMITER_URL", url.to_string())],
            ),
        )
        .unwrap();
        let a = attempt(20 + index as u8, 2, 10_000);
        assert_eq!(broken.consume(&a).await, Err(LimiterUnavailable));
        assert_eq!(broken.outcomes().unavailable, 1);
        assert_eq!(f.limiter().consume(&a).await, Ok(Admission::Allowed));
        assert!(matches!(
            f.limiter().consume(&a).await,
            Ok(Admission::Limited { .. })
        ));
    }
}

#[tokio::test]
async fn private_ca_tls_verifies_chain_and_hostname_before_admission() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    let mut url = url::Url::parse(&variable("DARKHORSE_REDIS_LIMITER_URL")).unwrap();
    url.set_scheme("rediss").unwrap();
    url.set_host(Some("localhost")).unwrap();
    url.set_port(Some(
        variable("DARKHORSE_TEST_REDIS_TLS_PORT").parse().unwrap(),
    ))
    .unwrap();
    let ca = variable("DARKHORSE_TEST_REDIS_CA_PEM");
    let verified = RedisLimiter::new(
        f.store.clone(),
        settings_with(
            false,
            vec![
                ("DARKHORSE_REDIS_LIMITER_URL", url.to_string()),
                ("DARKHORSE_REDIS_LIMITER_CA_PEM", ca.clone()),
            ],
        ),
    )
    .unwrap();
    let a = attempt(10, 2, 10_000);
    assert_eq!(verified.consume(&a).await, Ok(Admission::Allowed));
    let untrusted = RedisLimiter::new(
        f.store.clone(),
        settings_with(
            false,
            vec![("DARKHORSE_REDIS_LIMITER_URL", url.to_string())],
        ),
    )
    .unwrap();
    assert_eq!(untrusted.consume(&a).await, Err(LimiterUnavailable));
    url.set_host(Some("127.0.0.1")).unwrap();
    let wrong_name = RedisLimiter::new(
        f.store.clone(),
        settings_with(
            false,
            vec![
                ("DARKHORSE_REDIS_LIMITER_URL", url.to_string()),
                ("DARKHORSE_REDIS_LIMITER_CA_PEM", ca),
            ],
        ),
    )
    .unwrap();
    assert_eq!(wrong_name.consume(&a).await, Err(LimiterUnavailable));
    assert_eq!(verified.consume(&a).await, Ok(Admission::Allowed));
    assert!(matches!(
        verified.consume(&a).await,
        Ok(Admission::Limited { .. })
    ));
}

struct FenceAfterCharge<'a> {
    inner: &'a PostgresStore,
    reads: std::sync::atomic::AtomicUsize,
}
impl EnforcementAuthority for FenceAfterCharge<'_> {
    async fn read(&self) -> Result<Enforcement, LimiterUnavailable> {
        if self
            .reads
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            > 0
        {
            let mut nonce = [0; 16];
            getrandom::fill(&mut nonce).unwrap();
            self.inner.fence(nonce).await?;
        }
        self.inner.read().await
    }
}
#[tokio::test]
async fn durable_fence_between_charge_and_return_rejects_the_attempt_and_stale_writes() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    let authority = FenceAfterCharge {
        inner: &f.store,
        reads: std::sync::atomic::AtomicUsize::new(0),
    };
    let a = attempt(11, 2, 10_000);
    let before = f.store.read().await.unwrap();
    let snapshot = f.counters.snapshot(before, &a).await.unwrap();
    assert_eq!(
        shared_limiting::consume(&authority, &f.counters, &a).await,
        Err(LimiterUnavailable)
    );
    f.activate().await;
    let next = match darkhorse_domain::limiting::plan(&a, &snapshot.counters, snapshot.redis_ms)
        .unwrap()
    {
        darkhorse_domain::limiting::Plan::Charge(c) => c,
        _ => unreachable!(),
    };
    assert!(
        f.counters
            .apply(before, &a, &snapshot, &next, snapshot.redis_ms)
            .await
            .is_err()
    );
    assert_eq!(f.limiter().consume(&a).await, Ok(Admission::Allowed));
}

#[tokio::test]
async fn interrupted_cleanup_leaves_untrusted_state_instead_of_free_allowance() {
    let _serial = SERIAL.lock().await;
    let mut f = Fixture::new().await;
    f.activate().await;
    let now = f.store.read().await.unwrap().now_ms;
    let mut fill = redis::cmd("HSET");
    fill.arg("darkhorse:limiter:v1").arg("_count").arg(16384);
    for index in 0..16384 {
        fill.arg(format!("b:{index:064x}"))
            .arg(format!("1,1000,1,{0},{0}", now - 2000));
    }
    fill.query_async::<i64>(&mut f.admin).await.unwrap();
    redis::cmd("ACL")
        .arg("SETUSER")
        .arg("darkhorse-limiter")
        .arg("-hset")
        .query_async::<()>(&mut f.admin)
        .await
        .unwrap();
    let result = f.limiter().consume(&attempt(12, 1, 1000)).await;
    redis::cmd("ACL")
        .arg("SETUSER")
        .arg("darkhorse-limiter")
        .arg("+hset")
        .query_async::<()>(&mut f.admin)
        .await
        .unwrap();
    assert_eq!(result, Err(LimiterUnavailable));
    let length: i64 = redis::cmd("HLEN")
        .arg("darkhorse:limiter:v1")
        .query_async(&mut f.admin)
        .await
        .unwrap();
    assert!(
        length < 16389,
        "cleanup removed records before the injected metadata failure"
    );
    assert_eq!(
        f.limiter().consume(&attempt(12, 1, 1000)).await,
        Err(LimiterUnavailable)
    );
}

#[tokio::test]
async fn steady_state_admissions_expose_only_aggregate_outcomes() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    let limiter = f.limiter();
    let a = attempt(13, 110, 30_000);
    for _ in 0..10 {
        assert_eq!(limiter.consume(&a).await, Ok(Admission::Allowed));
    }
    let mut times = Vec::new();
    for _ in 0..100 {
        let start = std::time::Instant::now();
        assert_eq!(limiter.consume(&a).await, Ok(Admission::Allowed));
        times.push(start.elapsed().as_micros());
    }
    assert!(matches!(
        limiter.consume(&a).await,
        Ok(Admission::Limited { .. })
    ));
    f.fence().await;
    assert_eq!(limiter.consume(&a).await, Err(LimiterUnavailable));
    let outcomes = limiter.outcomes();
    assert_eq!(
        (outcomes.allowed, outcomes.limited, outcomes.unavailable),
        (110, 1, 1)
    );
    times.sort_unstable();
    println!(
        "Development limiter sample: 100 sequential warm admissions; microseconds p50={}, p95={}, p99={}. Includes two authoritative PostgreSQL reads. This is not a capacity benchmark.",
        times[49], times[94], times[98]
    );
}

#[tokio::test]
async fn separate_processes_share_one_budget_without_shared_connection_pools() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    sqlx::query("CREATE TABLE limiter_test_barrier (worker integer PRIMARY KEY)")
        .execute(&f.pool)
        .await
        .unwrap();
    let executable = std::env::current_exe().unwrap();
    let mut children = Vec::new();
    for worker in [1, 2] {
        children.push(
            std::process::Command::new(&executable)
                .args(["multiprocess_worker", "--exact", "--ignored", "--nocapture"])
                .env("DARKHORSE_TEST_LIMITER_WORKER", worker.to_string())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }
    let mut admitted = 0;
    for child in children {
        let result = child.wait_with_output().unwrap();
        assert!(result.status.success(), "independent limiter worker failed");
        let stdout = String::from_utf8(result.stdout).unwrap();
        admitted += stdout
            .lines()
            .find_map(|line| line.strip_prefix("worker_admitted="))
            .unwrap()
            .parse::<usize>()
            .unwrap();
    }
    assert_eq!(admitted, 5);
    assert!(matches!(
        f.limiter().consume(&attempt(22, 5, 30_000)).await,
        Ok(Admission::Limited { .. })
    ));
    sqlx::query("DROP TABLE limiter_test_barrier")
        .execute(&f.pool)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "executed by the separate-process parent scenario with disposable infrastructure"]
async fn multiprocess_worker() {
    let worker = variable("DARKHORSE_TEST_LIMITER_WORKER")
        .parse::<i32>()
        .unwrap();
    assert!([1, 2].contains(&worker));
    let pool = PgPool::connect(&variable("DARKHORSE_TEST_DATABASE_URL"))
        .await
        .unwrap();
    sqlx::query("INSERT INTO limiter_test_barrier(worker) VALUES($1)")
        .bind(worker)
        .execute(&pool)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let ready: i64 = sqlx::query_scalar("SELECT count(*) FROM limiter_test_barrier")
                .fetch_one(&pool)
                .await
                .unwrap();
            if ready == 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("both independent workers must join the barrier");
    let limiter = RedisLimiter::new(PostgresStore::from_pool(pool), settings(false)).unwrap();
    let mut admitted = 0;
    for _ in 0..20 {
        if limiter.consume(&attempt(22, 5, 30_000)).await == Ok(Admission::Allowed) {
            admitted += 1;
        }
    }
    println!("worker_admitted={admitted}");
}
