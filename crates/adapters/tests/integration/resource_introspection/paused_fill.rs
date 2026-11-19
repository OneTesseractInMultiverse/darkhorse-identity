use super::*;
use darkhorse_adapters::redis_computation_cache::RedisComputationCache;
use std::time::Duration;

// Gate only the fill. Both lookup and storage use the real Redis adapter.
struct PausedFill {
    redis: Arc<RedisComputationCache>,
    entered: Notify,
    resume: Notify,
}

impl AuthorizationComputationCache for PausedFill {
    fn lookup(
        &self,
        key: ComputationCacheKey,
    ) -> ComputationCacheFuture<'_, ComputationCacheLookup> {
        self.redis.lookup(key)
    }

    fn store(
        &self,
        key: ComputationCacheKey,
        entry: ComputationCacheEntry,
    ) -> ComputationCacheFuture<'_, ()> {
        Box::pin(async move {
            self.entered.notify_one();
            self.resume.notified().await;
            self.redis.store(key, entry).await;
        })
    }
}

pub(super) async fn warm_cache(
    fixture_pool: &PgPool,
    redis: Arc<RedisComputationCache>,
    probe: Probe,
) -> ActiveResourceToken {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(2))
        .connect_with((*fixture_pool.connect_options()).clone())
        .await
        .unwrap();
    let cache = Arc::new(PausedFill {
        redis,
        entered: Notify::new(),
        resume: Notify::new(),
    });
    let store = PostgresStore::from_pool(pool.clone()).with_computation_cache(cache.clone());
    let check_connection = async {
        tokio::time::timeout(Duration::from_secs(5), cache.entered.notified())
            .await
            .expect("introspection must reach the cache fill");
        let query = sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&pool)
            .await;
        cache.resume.notify_one();
        query
    };
    let (active, query) = tokio::join!(store.introspect_resource(probe, ISSUER), check_connection);
    pool.close().await;
    assert_eq!(
        query.expect("paused cache fill must release the only PostgreSQL connection"),
        1
    );
    active.unwrap().unwrap()
}
