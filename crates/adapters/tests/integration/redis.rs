use darkhorse_adapters::{
    redis_computation_cache::RedisComputationCache,
    redis_configuration,
    redis_infrastructure::{ProbeFailure, RedisInfrastructure},
};
use darkhorse_application::resource_servers::*;
use darkhorse_domain::identity::*;
use redis::aio::MultiplexedConnection;
use std::time::Duration;
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
fn variable(name: &str) -> String {
    std::env::var(name).expect("use make test-redis for disposable infrastructure")
}
fn configuration(overrides: Vec<(&str, String)>) -> redis_configuration::RedisSettings {
    let mut values = vec![
        (
            "DARKHORSE_REDIS_CACHE_URL",
            variable("DARKHORSE_REDIS_CACHE_URL"),
        ),
        (
            "DARKHORSE_REDIS_LIMITER_URL",
            variable("DARKHORSE_REDIS_LIMITER_URL"),
        ),
        ("DARKHORSE_REDIS_INSECURE", "true".to_owned()),
    ];
    values.extend(overrides);
    redis_configuration::load(envbind::MapEnvironment::from_pairs(values)).unwrap()
}
fn infrastructure(overrides: Vec<(&str, String)>) -> RedisInfrastructure {
    RedisInfrastructure::new(configuration(overrides)).unwrap()
}
async fn connection(name: &str) -> MultiplexedConnection {
    redis::Client::open(variable(name))
        .unwrap()
        .get_multiplexed_async_connection()
        .await
        .unwrap()
}
async fn configure(connection: &mut MultiplexedConnection, key: &str, value: &str) {
    redis::cmd("CONFIG")
        .arg("SET")
        .arg(key)
        .arg(value)
        .query_async::<()>(connection)
        .await
        .unwrap();
}
async fn container(operation: &str, role: &str) {
    let name = variable(&format!(
        "DARKHORSE_TEST_REDIS_{}_CONTAINER",
        role.to_ascii_uppercase()
    ));
    assert!(name.starts_with("darkhorse-redis-test-") && name.ends_with(role));
    let status = std::process::Command::new("docker")
        .args([operation, &name])
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn separate_replicas_observe_the_same_role_identity_and_runtime_users_cannot_mutate() {
    let _serial = SERIAL.lock().await;
    let left = infrastructure(vec![]);
    let right = infrastructure(vec![]);
    let (left, right) = tokio::join!(left.inspect(), right.inspect());
    assert_eq!(
        left.cache.as_ref().unwrap().run_id,
        right.cache.unwrap().run_id
    );
    assert_eq!(
        left.limiter.as_ref().unwrap().run_id,
        right.limiter.unwrap().run_id
    );
    assert_ne!(left.cache.unwrap().run_id, left.limiter.unwrap().run_id);
    for role in ["CACHE", "LIMITER"] {
        let mut connection = connection(&format!("DARKHORSE_REDIS_{role}_URL")).await;
        for command in [
            redis::cmd("SET").arg("forbidden").arg("value"),
            redis::cmd("CONFIG")
                .arg("SET")
                .arg("maxmemory-policy")
                .arg("allkeys-lru"),
        ] {
            assert!(
                command
                    .query_async::<String>(&mut connection)
                    .await
                    .is_err()
            );
        }
    }
}

#[tokio::test]
async fn authentication_permission_and_policy_failures_are_independent_and_redacted() {
    let _serial = SERIAL.lock().await;
    let mut bad = url::Url::parse(&variable("DARKHORSE_REDIS_CACHE_URL")).unwrap();
    bad.set_password(Some("wrong-cache-password")).unwrap();
    let status = infrastructure(vec![("DARKHORSE_REDIS_CACHE_URL", bad.to_string())])
        .inspect()
        .await;
    assert_eq!(status.cache, Err(ProbeFailure::Unavailable));
    assert!(status.limiter.is_ok());
    let pool = infrastructure(vec![]);
    let mut admin = connection("DARKHORSE_REDIS_LIMITER_ADMIN_URL").await;
    redis::cmd("ACL")
        .arg("SETUSER")
        .arg("darkhorse-limiter")
        .arg("-info")
        .query_async::<()>(&mut admin)
        .await
        .unwrap();
    let denied = pool.inspect().await;
    redis::cmd("ACL")
        .arg("SETUSER")
        .arg("darkhorse-limiter")
        .arg("+info")
        .query_async::<()>(&mut admin)
        .await
        .unwrap();
    assert_eq!(denied.limiter, Err(ProbeFailure::Unavailable));
    assert!(denied.cache.is_ok());
    assert!(pool.inspect().await.limiter.is_ok());
    configure(&mut admin, "maxmemory-policy", "allkeys-lru").await;
    let unsafe_state = pool.inspect().await;
    configure(&mut admin, "maxmemory-policy", "noeviction").await;
    assert_eq!(unsafe_state.limiter, Err(ProbeFailure::UnsafeConfiguration));
}

#[tokio::test]
async fn policy_cache_uses_only_its_scoped_runtime_commands_and_returns_a_verified_entry() {
    let _serial = SERIAL.lock().await;
    let resource = ResourceId::from_u128(uuid::Uuid::new_v4().as_u128()).unwrap();
    let key = ComputationCacheKey {
        resource,
        token_digest: [0x41; 32],
    };
    let context = ComputationCacheContext {
        token_digest: key.token_digest,
        resource,
        principal: PrincipalId::from_u128(1).unwrap(),
        client: ClientId::from_u128(2).unwrap(),
        credential: CredentialId::from_u128(3).unwrap(),
        principal_epoch: 4,
        scopes: vec!["read".into()],
        capability_ceiling: [CapabilityId::from_u128(5).unwrap()].into(),
    };
    let cache = RedisComputationCache::from_settings(&configuration(vec![]), &[0x29; 32]).unwrap();
    let missing = cache.lookup(key).await;
    assert!(missing.entry.is_none());
    assert!(missing.may_store());
    cache
        .store(
            key,
            ComputationCacheEntry {
                policy_revision: 7,
                context: context.clone(),
                payload: br#"{"projection":true}"#.to_vec(),
            },
        )
        .await;
    drop(missing);

    let hit = cache.lookup(key).await;
    assert!(!hit.may_store());
    let entry = hit.entry.as_ref().unwrap();
    assert!(entry.applies_to(7, &entry.context));
    assert_eq!(entry.payload, br#"{"projection":true}"#);
    let outcomes = cache.outcomes();
    assert_eq!(outcomes.hit, 1);
    assert_eq!(outcomes.miss, 1);
    assert_eq!(outcomes.stored, 1);

    let redis_key = format!(
        "darkhorse:authorization:v1:{}:{}",
        uuid::Uuid::from_u128(resource.as_u128()),
        "41".repeat(32)
    );
    let mut admin = connection("DARKHORSE_REDIS_CACHE_ADMIN_URL").await;
    redis::cmd("SET")
        .arg(&redis_key)
        .arg("malformed cache data")
        .arg("PX")
        .arg(60_000)
        .query_async::<()>(&mut admin)
        .await
        .unwrap();
    let invalid = cache.lookup(key).await;
    assert!(invalid.entry.is_none());
    assert!(invalid.may_store());
    drop(invalid);
    cache
        .store(
            key,
            ComputationCacheEntry {
                policy_revision: 7,
                context,
                payload: br#"{"projection":true}"#.to_vec(),
            },
        )
        .await;
    let repaired = cache.lookup(key).await;
    assert!(repaired.entry.is_some());
    let outcomes = cache.outcomes();
    assert_eq!(outcomes.hit, 2);
    assert_eq!(outcomes.miss, 2);
    assert_eq!(outcomes.invalid, 1);
    assert_eq!(outcomes.stored, 2);
}

#[tokio::test]
async fn cache_outage_is_a_bounded_non_storing_miss_and_recovers_after_unpause() {
    let _serial = SERIAL.lock().await;
    let resource = ResourceId::from_u128(uuid::Uuid::new_v4().as_u128()).unwrap();
    let key = ComputationCacheKey {
        resource,
        token_digest: [0x42; 32],
    };
    let context = ComputationCacheContext {
        token_digest: key.token_digest,
        resource,
        principal: PrincipalId::from_u128(1).unwrap(),
        client: ClientId::from_u128(2).unwrap(),
        credential: CredentialId::from_u128(3).unwrap(),
        principal_epoch: 4,
        scopes: vec!["read".into()],
        capability_ceiling: [CapabilityId::from_u128(5).unwrap()].into(),
    };
    let cache = RedisComputationCache::from_settings(&configuration(vec![]), &[0x29; 32]).unwrap();
    container("pause", "cache").await;
    let unavailable = tokio::time::timeout(Duration::from_secs(1), cache.lookup(key))
        .await
        .unwrap();
    assert!(unavailable.entry.is_none());
    assert!(!unavailable.may_store());
    drop(unavailable);
    cache
        .store(
            key,
            ComputationCacheEntry {
                policy_revision: 1,
                context,
                payload: br#"{"projection":true}"#.to_vec(),
            },
        )
        .await;
    assert_eq!(cache.outcomes().unavailable, 1);
    assert_eq!(cache.outcomes().store_failed, 1);

    container("unpause", "cache").await;
    let recovered = cache.lookup(key).await;
    assert!(recovered.entry.is_none());
    assert!(recovered.may_store());
    assert_eq!(cache.outcomes().miss, 1);
}

#[tokio::test]
async fn cache_eviction_and_limiter_memory_pressure_do_not_share_state() {
    let _serial = SERIAL.lock().await;
    let pool = infrastructure(vec![]);
    let mut cache = connection("DARKHORSE_REDIS_CACHE_ADMIN_URL").await;
    let mut limiter = connection("DARKHORSE_REDIS_LIMITER_ADMIN_URL").await;
    redis::cmd("SET")
        .arg("retained-marker")
        .arg("retained")
        .query_async::<()>(&mut limiter)
        .await
        .unwrap();
    configure(&mut cache, "maxmemory", "2097152").await;
    for i in 0..64 {
        redis::cmd("SET")
            .arg(format!("pressure:{i}"))
            .arg(vec![b'x'; 131072])
            .query_async::<()>(&mut cache)
            .await
            .unwrap();
    }
    let info: String = redis::cmd("INFO")
        .arg("stats")
        .query_async(&mut cache)
        .await
        .unwrap();
    let evicted: u64 = info
        .lines()
        .find_map(|line| line.strip_prefix("evicted_keys:"))
        .unwrap()
        .parse()
        .unwrap();
    configure(&mut cache, "maxmemory", "67108864").await;
    assert!(evicted > 0);
    let retained: String = redis::cmd("GET")
        .arg("retained-marker")
        .query_async(&mut limiter)
        .await
        .unwrap();
    assert_eq!(retained, "retained");
    assert!(pool.inspect().await.limiter.is_ok());
    configure(&mut limiter, "maxmemory", "1").await;
    let pressure = pool.inspect().await;
    configure(&mut limiter, "maxmemory", "67108864").await;
    assert_eq!(pressure.limiter, Err(ProbeFailure::UnsafeConfiguration));
    assert!(pressure.cache.is_ok());
}

#[tokio::test]
async fn stalled_connections_time_out_and_restart_changes_identity_without_an_admission_claim() {
    let _serial = SERIAL.lock().await;
    let pool = infrastructure(vec![]);
    let initial = pool.inspect().await.limiter.unwrap().run_id;
    container("pause", "limiter").await;
    let result = tokio::time::timeout(Duration::from_secs(2), pool.inspect()).await;
    container("unpause", "limiter").await;
    assert_eq!(result.unwrap().limiter, Err(ProbeFailure::Unavailable));
    assert!(pool.inspect().await.limiter.is_ok());
    container("restart", "limiter").await;
    let mut observed = None;
    for _ in 0..30 {
        if let Ok(id) = pool.inspect().await.limiter {
            observed = Some(id.run_id);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_ne!(observed.unwrap(), initial);
}

#[tokio::test]
async fn verified_tls_never_downgrades_to_a_plaintext_endpoint() {
    let _serial = SERIAL.lock().await;
    let url = variable("DARKHORSE_REDIS_CACHE_URL").replacen("redis:", "rediss:", 1);
    let status = infrastructure(vec![("DARKHORSE_REDIS_CACHE_URL", url)])
        .inspect()
        .await;
    assert_eq!(status.cache, Err(ProbeFailure::Unavailable));
    assert!(status.limiter.is_ok());
}
