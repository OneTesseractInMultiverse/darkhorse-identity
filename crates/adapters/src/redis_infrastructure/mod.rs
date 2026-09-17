//! Independent, bounded connection pools and read-only infrastructure diagnostics.
//! A reachable limiter is not evidence that shared admission enforcement is ready.
use crate::redis_configuration::{Endpoint, RedisSettings};
use redis::{AsyncConnectionConfig, Client, aio::MultiplexedConnection};
use std::time::Duration;
use tokio::sync::Mutex;
mod identity;

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

struct Pool {
    client: Client,
    slots: Vec<Mutex<Option<MultiplexedConnection>>>,
    deadline: Duration,
}
impl Pool {
    fn new(endpoint: Endpoint) -> Result<Self, ProbeFailure> {
        let client =
            Client::open(endpoint.url.as_str()).map_err(|_| ProbeFailure::UnsafeConfiguration)?;
        Ok(Self {
            client,
            slots: (0..endpoint.connections)
                .map(|_| Mutex::new(None))
                .collect(),
            deadline: Duration::from_millis(endpoint.timeout_ms.into()),
        })
    }
    async fn inspect(&self, role: Role) -> Result<Identity, ProbeFailure> {
        let mut slot = self
            .slots
            .iter()
            .find_map(|slot| slot.try_lock().ok())
            .ok_or(ProbeFailure::Unavailable)?;
        let result = tokio::time::timeout(self.deadline, self.inspect_slot(&mut slot, role))
            .await
            .unwrap_or(Err(ProbeFailure::Unavailable));
        if result.is_err() {
            slot.take();
        }
        result
    }
    async fn inspect_slot(
        &self,
        slot: &mut Option<MultiplexedConnection>,
        role: Role,
    ) -> Result<Identity, ProbeFailure> {
        if slot.is_none() {
            let config = AsyncConnectionConfig::new()
                .set_pipeline_buffer_size(1)
                .set_concurrency_limit(1)
                .set_connection_timeout(Some(self.deadline))
                .set_response_timeout(Some(self.deadline));
            *slot = Some(
                self.client
                    .get_multiplexed_async_connection_with_config(&config)
                    .await
                    .map_err(|_| ProbeFailure::Unavailable)?,
            );
        }
        let connection = slot.as_mut().ok_or(ProbeFailure::Unavailable)?;
        let (pong, server, memory, replication): (String, String, String, String) = redis::pipe()
            .cmd("PING")
            .cmd("INFO")
            .arg("server")
            .cmd("INFO")
            .arg("memory")
            .cmd("INFO")
            .arg("replication")
            .query_async(connection)
            .await
            .map_err(|_| ProbeFailure::Unavailable)?;
        identity::validate(role, &pong, &server, &memory, &replication)
    }
}
#[cfg(test)]
#[path = "../../tests/unit/redis_infrastructure/mod.rs"]
mod tests;
