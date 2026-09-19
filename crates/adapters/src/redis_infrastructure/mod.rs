//! Independent, bounded connection pools and read-only infrastructure diagnostics.
//! A reachable limiter is not evidence that shared admission enforcement is ready.
use crate::redis_configuration::{Endpoint, RedisSettings};
use redis::{AsyncConnectionConfig, Client, aio::MultiplexedConnection};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
pub(crate) mod identity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Cache,
    Limiter,
}
impl Role {
    fn eviction_policy(self) -> &'static str {
        match self {
            Self::Cache => "allkeys-lru",
            Self::Limiter => "noeviction",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeFailure {
    Unavailable,
    UnsafeConfiguration,
    SharedInstance,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub run_id: String,
    pub memory_limit_bytes: u64,
    pub used_memory_bytes: u64,
}
pub struct InfrastructureStatus {
    pub cache: Result<Identity, ProbeFailure>,
    pub limiter: Result<Identity, ProbeFailure>,
}
pub struct RedisInfrastructure {
    cache: Pool,
    limiter: Pool,
}
impl RedisInfrastructure {
    pub fn new(settings: RedisSettings) -> Result<Self, ProbeFailure> {
        Ok(Self {
            cache: Pool::new(settings.cache)?,
            limiter: Pool::new(settings.limiter)?,
        })
    }
    pub async fn inspect(&self) -> InfrastructureStatus {
        let (cache, limiter) = tokio::join!(
            self.cache.inspect(Role::Cache),
            self.limiter.inspect(Role::Limiter)
        );
        separate_roles(cache, limiter)
    }
}
fn separate_roles(
    cache: Result<Identity, ProbeFailure>,
    limiter: Result<Identity, ProbeFailure>,
) -> InfrastructureStatus {
    if matches!((&cache,&limiter),(Ok(c),Ok(l)) if c.run_id==l.run_id) {
        InfrastructureStatus {
            cache: Err(ProbeFailure::SharedInstance),
            limiter: Err(ProbeFailure::SharedInstance),
        }
    } else {
        InfrastructureStatus { cache, limiter }
    }
}

struct CachedConnection {
    value: MultiplexedConnection,
    last_used: Instant,
}
// Retire before the supplied Redis configuration's 60-second idle timeout.
fn reusable(idle: Duration) -> bool {
    idle < Duration::from_secs(30)
}
pub(crate) struct Pool {
    client: Client,
    slots: Vec<Mutex<Option<CachedConnection>>>,
    deadline: Duration,
}
impl Pool {
    pub(crate) fn new(endpoint: Endpoint) -> Result<Self, ProbeFailure> {
        let client = if let Some(pem) = endpoint.ca_pem {
            Client::build_with_tls(
                endpoint.url.as_str(),
                redis::TlsCertificates {
                    client_tls: None,
                    root_cert: Some(pem.into_bytes()),
                },
            )
        } else {
            Client::open(endpoint.url.as_str())
        }
        .map_err(|_| ProbeFailure::UnsafeConfiguration)?;
        Ok(Self {
            client,
            slots: (0..endpoint.connections)
                .map(|_| Mutex::new(None))
                .collect(),
            deadline: Duration::from_millis(endpoint.timeout_ms.into()),
        })
    }
    pub(crate) async fn execute<T, F, Fut>(&self, operation: F) -> Result<T, ProbeFailure>
    where
        F: FnOnce(MultiplexedConnection) -> Fut,
        Fut: std::future::Future<Output = Result<T, ProbeFailure>>,
    {
        let mut slot = self
            .slots
            .iter()
            .find_map(|slot| slot.try_lock().ok())
            .ok_or(ProbeFailure::Unavailable)?;
        let result = tokio::time::timeout(self.deadline, async {
            self.connect_slot(&mut slot).await?;
            operation(
                slot.as_ref()
                    .ok_or(ProbeFailure::Unavailable)?
                    .value
                    .clone(),
            )
            .await
        })
        .await
        .unwrap_or(Err(ProbeFailure::Unavailable));
        if result.is_err() {
            slot.take();
        } else if let Some(cached) = slot.as_mut() {
            cached.last_used = Instant::now();
        }
        result
    }
    async fn connect_slot(&self, slot: &mut Option<CachedConnection>) -> Result<(), ProbeFailure> {
        if slot
            .as_ref()
            .is_some_and(|cached| !reusable(cached.last_used.elapsed()))
        {
            slot.take();
        }
        if slot.is_none() {
            let config = AsyncConnectionConfig::new()
                .set_pipeline_buffer_size(1)
                .set_concurrency_limit(1)
                .set_connection_timeout(Some(self.deadline))
                .set_response_timeout(Some(self.deadline));
            let value = self
                .client
                .get_multiplexed_async_connection_with_config(&config)
                .await
                .map_err(|_| ProbeFailure::Unavailable)?;
            *slot = Some(CachedConnection {
                value,
                last_used: Instant::now(),
            });
        }
        Ok(())
    }
    async fn inspect(&self, role: Role) -> Result<Identity, ProbeFailure> {
        self.execute(|mut connection| async move {
            let (pong, server, memory, replication): (String, String, String, String) =
                redis::pipe()
                    .cmd("PING")
                    .cmd("INFO")
                    .arg("server")
                    .cmd("INFO")
                    .arg("memory")
                    .cmd("INFO")
                    .arg("replication")
                    .query_async(&mut connection)
                    .await
                    .map_err(|_| ProbeFailure::Unavailable)?;
            identity::validate(role, &pong, &server, &memory, &replication)
        })
        .await
    }
}
#[cfg(test)]
#[path = "../../tests/unit/redis_infrastructure/mod.rs"]
mod tests;
