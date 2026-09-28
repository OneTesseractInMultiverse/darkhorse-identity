//! Bounded Redis storage for versioned resource-policy projections.
mod wire;

use crate::{
    redis_configuration::{Endpoint, RedisSettings},
    redis_infrastructure::{Pool, ProbeFailure},
};
use darkhorse_application::resource_servers::{
    AuthorizationComputationCache, ComputationCacheEntry, ComputationCacheFuture,
    ComputationCacheKey, ComputationCacheLease, ComputationCacheLookup,
};
use hmac::{Hmac, KeyInit, Mac};
use redis::aio::MultiplexedConnection;
use sha2::{Digest, Sha256};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Mutex, OwnedMutexGuard};
use zeroize::Zeroize;

type HmacSha256 = Hmac<Sha256>;
const KEY_PREFIX: &str = "darkhorse:authorization:v1";
const TTL_MS: u64 = 60_000;
const CACHE_DEADLINE_MS: u16 = 50;
const COALESCE_WAIT_MS: u64 = 10;
const COALESCE_SHARDS: usize = 32;

#[derive(Default)]
struct Counters {
    hit: AtomicU64,
    miss: AtomicU64,
    invalid: AtomicU64,
    unavailable: AtomicU64,
    stored: AtomicU64,
    store_failed: AtomicU64,
    coalesced: AtomicU64,
    coalesce_timeout: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcomes {
    pub hit: u64,
    pub miss: u64,
    pub invalid: u64,
    pub unavailable: u64,
    pub stored: u64,
    pub store_failed: u64,
    pub coalesced: u64,
    pub coalesce_timeout: u64,
}

impl Counters {
    fn snapshot(&self) -> Outcomes {
        Outcomes {
            hit: self.hit.load(Ordering::Relaxed),
            miss: self.miss.load(Ordering::Relaxed),
            invalid: self.invalid.load(Ordering::Relaxed),
            unavailable: self.unavailable.load(Ordering::Relaxed),
            stored: self.stored.load(Ordering::Relaxed),
            store_failed: self.store_failed.load(Ordering::Relaxed),
            coalesced: self.coalesced.load(Ordering::Relaxed),
            coalesce_timeout: self.coalesce_timeout.load(Ordering::Relaxed),
        }
    }
}

struct Lease {
    _guard: OwnedMutexGuard<()>,
}
impl ComputationCacheLease for Lease {}

pub struct RedisComputationCache {
    pool: Pool,
    mac_key: [u8; 32],
    shards: Vec<Arc<Mutex<()>>>,
    outcomes: Counters,
}

impl RedisComputationCache {
    pub fn from_settings(
        settings: &RedisSettings,
        root_key: &[u8; 32],
    ) -> Result<Self, ProbeFailure> {
        Self::new(settings.cache.clone(), root_key)
    }

    pub fn new(mut endpoint: Endpoint, root_key: &[u8; 32]) -> Result<Self, ProbeFailure> {
        endpoint.timeout_ms = endpoint.timeout_ms.min(CACHE_DEADLINE_MS);
        let mut mac =
            HmacSha256::new_from_slice(root_key).map_err(|_| ProbeFailure::UnsafeConfiguration)?;
        mac.update(b"darkhorse:resource-policy-cache:integrity:v1");
        let mac_key = mac.finalize().into_bytes().into();
        Ok(Self {
            pool: Pool::new(endpoint)?,
            mac_key,
            shards: (0..COALESCE_SHARDS)
                .map(|_| Arc::new(Mutex::new(())))
                .collect(),
            outcomes: Counters::default(),
        })
    }

    pub fn outcomes(&self) -> Outcomes {
        self.outcomes.snapshot()
    }

    async fn read(&self, key: &ComputationCacheKey) -> Result<wire::Read, ProbeFailure> {
        let redis_key = wire::key(key);
        let raw = self
            .pool
            .execute(|mut connection| async move { get_bounded(&redis_key, &mut connection).await })
            .await?;
        match raw {
            None => Ok(wire::Read::Miss),
            Some(bytes) => match wire::decode(key, &bytes, &self.mac_key) {
                Ok(entry) => Ok(wire::Read::Hit(entry)),
                Err(()) => Ok(wire::Read::Invalid),
            },
        }
    }

    fn shard(&self, key: &ComputationCacheKey) -> usize {
        let digest = Sha256::digest(
            [
                key.resource.as_u128().to_be_bytes().as_slice(),
                &key.token_digest,
            ]
            .concat(),
        );
        usize::from(digest[0]) % self.shards.len()
    }
}

impl Drop for RedisComputationCache {
    fn drop(&mut self) {
        self.mac_key.zeroize();
    }
}

impl AuthorizationComputationCache for RedisComputationCache {
    fn lookup(
        &self,
        key: ComputationCacheKey,
    ) -> ComputationCacheFuture<'_, ComputationCacheLookup> {
        Box::pin(async move {
            match self.read(&key).await {
                Ok(wire::Read::Hit(entry)) => {
                    self.outcomes.hit.fetch_add(1, Ordering::Relaxed);
                    return ComputationCacheLookup::new(Some(entry), None);
                }
                Ok(wire::Read::Invalid) => {
                    self.outcomes.invalid.fetch_add(1, Ordering::Relaxed);
                }
                Ok(wire::Read::Miss) => {}
                Err(_) => {
                    self.outcomes.unavailable.fetch_add(1, Ordering::Relaxed);
                    return ComputationCacheLookup::new(None, None);
                }
            }
            self.outcomes.miss.fetch_add(1, Ordering::Relaxed);

            let shard = self.shard(&key);
            let lease = tokio::time::timeout(
                Duration::from_millis(COALESCE_WAIT_MS),
                Arc::clone(&self.shards[shard]).lock_owned(),
            )
            .await;
            let Ok(lease) = lease else {
                self.outcomes
                    .coalesce_timeout
                    .fetch_add(1, Ordering::Relaxed);
                return ComputationCacheLookup::new(None, None);
            };

            match self.read(&key).await {
                Ok(wire::Read::Hit(entry)) => {
                    self.outcomes.coalesced.fetch_add(1, Ordering::Relaxed);
                    ComputationCacheLookup::new(Some(entry), None)
                }
                Ok(wire::Read::Invalid) | Ok(wire::Read::Miss) => {
                    ComputationCacheLookup::new(None, Some(Box::new(Lease { _guard: lease })))
                }
                Err(_) => {
                    self.outcomes.unavailable.fetch_add(1, Ordering::Relaxed);
                    ComputationCacheLookup::new(None, None)
                }
            }
        })
    }

    fn store(
        &self,
        key: ComputationCacheKey,
        entry: ComputationCacheEntry,
    ) -> ComputationCacheFuture<'_, ()> {
        Box::pin(async move {
            let Ok(bytes) = wire::encode(&key, &entry, &self.mac_key) else {
                self.outcomes.store_failed.fetch_add(1, Ordering::Relaxed);
                return;
            };
            let redis_key = wire::key(&key);
            let result = self
                .pool
                .execute(|mut connection| async move {
                    set_expiring(&redis_key, &bytes, &mut connection).await
                })
                .await;
            if result.is_ok() {
                self.outcomes.stored.fetch_add(1, Ordering::Relaxed);
            } else {
                self.outcomes.store_failed.fetch_add(1, Ordering::Relaxed);
            }
        })
    }
}

async fn get_bounded(
    key: &str,
    connection: &mut MultiplexedConnection,
) -> Result<Option<Vec<u8>>, ProbeFailure> {
    let value: Option<Vec<u8>> = redis::cmd("GETRANGE")
        .arg(key)
        .arg(0)
        .arg(wire::MAX_VALUE_BYTES)
        .query_async(connection)
        .await
        .map_err(|_| ProbeFailure::Unavailable)?;
    Ok(value.filter(|bytes| !bytes.is_empty()))
}

async fn set_expiring(
    key: &str,
    value: &[u8],
    connection: &mut MultiplexedConnection,
) -> Result<(), ProbeFailure> {
    let _: String = redis::cmd("SET")
        .arg(key)
        .arg(value)
        .arg("PX")
        .arg(TTL_MS)
        .query_async(connection)
        .await
        .map_err(|_| ProbeFailure::Unavailable)?;
    Ok(())
}
