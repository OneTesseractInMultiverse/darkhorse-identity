//! Bounded atomic counter storage with durable authority checked on both sides.
mod metrics;
mod wire;
use crate::{
    postgres::PostgresStore,
    redis_configuration::RedisSettings,
    redis_infrastructure::{Pool, ProbeFailure, Role, identity},
};
use darkhorse_application::{
    limiting::{Admission, AttemptLimiter, LimiterUnavailable},
    shared_limiting::{self, Commit, CounterStore, Snapshot},
};
use darkhorse_domain::{
    limiter_recovery::{Enforcement, OPERATION_MS, ServerIdentity, activation_ready, trusted_time},
    limiting::{Attempt, Counter, MAX_TIME},
};
pub use metrics::Outcomes;
use redis::{Script, ScriptInvocation, aio::MultiplexedConnection};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
const KEY: &str = "darkhorse:limiter:v1";
const SCRIPT: &str = include_str!("atomic.lua");
pub const CAPACITY: usize = 16384;
#[derive(Clone)]
pub struct RedisCounters {
    pool: Arc<Pool>,
    script: Script,
}
pub struct RedisLimiter {
    authority: PostgresStore,
    counters: RedisCounters,
    slots: tokio::sync::Semaphore,
    outcomes: metrics::Counters,
}
impl RedisLimiter {
    pub fn new(
        authority: PostgresStore,
        settings: RedisSettings,
    ) -> Result<Self, LimiterUnavailable> {
        let concurrency = settings.limiter.connections.into();
        Ok(Self {
            authority,
            counters: RedisCounters::new(settings)?,
            slots: tokio::sync::Semaphore::new(concurrency),
            outcomes: metrics::Counters::default(),
        })
    }
    pub fn outcomes(&self) -> Outcomes {
        self.outcomes.snapshot()
    }
}
impl AttemptLimiter for RedisLimiter {
    async fn consume(&self, attempt: &Attempt) -> Result<Admission, LimiterUnavailable> {
        let result = self.bounded_consume(attempt).await;
        self.outcomes.record(&result);
        result
    }
}
impl RedisLimiter {
    async fn bounded_consume(&self, attempt: &Attempt) -> Result<Admission, LimiterUnavailable> {
        let _slot = self.slots.try_acquire().map_err(unavailable)?;
        let started = Instant::now();
        let duration = Duration::from_millis(OPERATION_MS);
        let result = tokio::time::timeout(
            duration,
            shared_limiting::consume(&self.authority, &self.counters, attempt),
        )
        .await
        .map_err(unavailable)?;
        if started.elapsed() > duration {
            return Err(LimiterUnavailable);
        }
        result
    }
}
impl RedisCounters {
    pub fn new(settings: RedisSettings) -> Result<Self, LimiterUnavailable> {
        Ok(Self {
            pool: Arc::new(Pool::new(settings.limiter).map_err(unavailable)?),
            script: Script::new(SCRIPT),
        })
    }
    pub async fn identify(&self) -> Result<ServerIdentity, LimiterUnavailable> {
        self.pool
            .execute(|mut connection| async move {
                let (server, memory, replication): (String, String, String) = redis::pipe()
                    .cmd("INFO")
                    .arg("server")
                    .cmd("INFO")
                    .arg("memory")
                    .cmd("INFO")
                    .arg("replication")
                    .query_async(&mut connection)
                    .await
                    .map_err(probe)?;
                let verified =
                    identity::validate(Role::Limiter, "PONG", &server, &memory, &replication)?;
                Ok(ServerIdentity {
                    run: wire::unhex(&verified.run_id).map_err(probe)?,
                    replication: wire::unhex(identity::field(&replication, "master_replid")?)
                        .map_err(probe)?,
                })
            })
            .await
            .map_err(unavailable)
    }
    pub async fn status(&self, state: Enforcement) -> Result<u64, LimiterUnavailable> {
        self.invoke(self.invocation("status", state)?).await
    }
    /// Requires operator Redis credentials; never clears an already initialized generation.
    pub async fn initialize(
        &self,
        state: Enforcement,
    ) -> Result<ServerIdentity, LimiterUnavailable> {
        activation_ready(state).map_err(unavailable)?;
        let identity = self.identify().await?;
        let bound = Enforcement {
            identity: Some(identity),
            ..state
        };
        let call = self.invocation("initialize", bound)?;
        let result: i64 = self.invoke(call).await?;
        completed(result)?;
        Ok(identity)
    }
    fn invocation(
        &self,
        operation: &str,
        state: Enforcement,
    ) -> Result<ScriptInvocation<'_>, LimiterUnavailable> {
        let id = state.identity.ok_or(LimiterUnavailable)?;
        let mut call = self.script.prepare_invoke();
        call.key(KEY)
            .arg(operation)
            .arg(wire::hex(&state.generation.nonce()))
            .arg(state.generation.epoch())
            .arg(wire::hex(&id.run))
            .arg(wire::hex(&id.replication))
            .arg(state.now_ms);
        Ok(call)
    }
    async fn invoke<T: redis::FromRedisValue>(
        &self,
        call: ScriptInvocation<'_>,
    ) -> Result<T, LimiterUnavailable> {
        self.pool
            .execute(|mut connection| async move { execute(call, &mut connection).await })
            .await
            .map_err(unavailable)
    }
}
async fn execute<T: redis::FromRedisValue>(
    call: ScriptInvocation<'_>,
    connection: &mut MultiplexedConnection,
) -> Result<T, ProbeFailure> {
    // redis-rs reloads only after an explicit NOSCRIPT (known not to have run).
    // It does not retry timeouts, disconnects, or other potentially committed errors.
    call.invoke_async(connection).await.map_err(probe)
}
fn unavailable<T>(_: T) -> LimiterUnavailable {
    LimiterUnavailable
}
fn probe<T>(_: T) -> ProbeFailure {
    ProbeFailure::Unavailable
}
fn field(key: &[u8; 32]) -> String {
    format!("b:{}", wire::hex(key))
}
fn snapshot(values: Vec<String>, length: usize) -> Result<Snapshot, LimiterUnavailable> {
    if values.len() != length + 2 {
        return Err(LimiterUnavailable);
    }
    Ok(Snapshot {
        redis_ms: values[0].parse().map_err(unavailable)?,
        previous_ms: values[1].parse().map_err(unavailable)?,
        counters: values[2..]
            .iter()
            .map(|v| wire::decode(v))
            .collect::<Result<_, _>>()?,
    })
}
fn valid_until(counters: &[Counter]) -> Result<u64, LimiterUnavailable> {
    counters
        .iter()
        .map(|c| {
            c.started_ms
                .checked_add(c.rule.window_ms().into())
                .filter(|&end| end <= MAX_TIME)
                .ok_or(LimiterUnavailable)
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min()
        .ok_or(LimiterUnavailable)
}
impl CounterStore for RedisCounters {
    async fn snapshot(
        &self,
        state: Enforcement,
        attempt: &Attempt,
    ) -> Result<Snapshot, LimiterUnavailable> {
        let mut call = self.invocation("snapshot", state)?;
        for budget in attempt.budgets() {
            call.arg(field(&budget.key));
        }
        snapshot(self.invoke(call).await?, attempt.budgets().len())
    }
    async fn apply(
        &self,
        state: Enforcement,
        attempt: &Attempt,
        snapshot: &Snapshot,
        counters: &[Counter],
        _now_ms: u64,
    ) -> Result<Commit, LimiterUnavailable> {
        if counters.len() != attempt.budgets().len() || snapshot.counters.len() != counters.len() {
            return Err(LimiterUnavailable);
        }
        let mut call = self.invocation("apply", state)?;
        call.arg(valid_until(counters)?).arg(snapshot.redis_ms);
        for ((budget, previous), next) in attempt
            .budgets()
            .iter()
            .zip(&snapshot.counters)
            .zip(counters)
        {
            call.arg(field(&budget.key))
                .arg(wire::encode(*previous))
                .arg(wire::encode(Some(*next)));
        }
        commit_reply(self.invoke::<i64>(call).await?)
    }

    async fn prune(&self, state: Enforcement) -> Result<(), LimiterUnavailable> {
        let (now, previous, cursor, values): (u64, u64, String, Vec<String>) =
            self.invoke(self.invocation("scan", state)?).await?;
        let expired = expired(state, now, previous, &values)?;
        let mut call = self.invocation("prune", state)?;
        call.arg(cursor).arg(now);
        for (key, value) in expired {
            call.arg(key).arg(value);
        }
        completed(self.invoke(call).await?)
    }
}
fn completed(reply: i64) -> Result<(), LimiterUnavailable> {
    if reply == 1 {
        Ok(())
    } else {
        Err(LimiterUnavailable)
    }
}
fn commit_reply(reply: i64) -> Result<Commit, LimiterUnavailable> {
    match reply {
        1 => Ok(Commit::Applied),
        0 => Ok(Commit::Conflict),
        2 => Ok(Commit::Full),
        _ => Err(LimiterUnavailable),
    }
}

fn expired(
    state: Enforcement,
    now: u64,
    previous: u64,
    values: &[String],
) -> Result<Vec<(&str, &str)>, LimiterUnavailable> {
    let now = trusted_time(state.now_ms, now, previous).map_err(unavailable)?;
    if !values.len().is_multiple_of(2) || values.len() > 2 * (CAPACITY + 5) {
        return Err(LimiterUnavailable);
    }
    let mut expired = Vec::new();
    for pair in values.chunks_exact(2).take(512) {
        if pair[0].starts_with('_') {
            continue;
        }
        let key = pair[0].strip_prefix("b:").ok_or(LimiterUnavailable)?;
        wire::unhex::<32>(key)?;
        let c = wire::decode(&pair[1])?.ok_or(LimiterUnavailable)?;
        let end = valid_until(&[c])?;
        // Validate stored policy/counter invariants before considering physical cleanup.
        let attempt = Attempt::new(vec![darkhorse_domain::limiting::Budget {
            key: [1; 32],
            rule: c.rule,
        }])
        .map_err(unavailable)?;
        darkhorse_domain::limiting::plan(&attempt, &[Some(c)], now).map_err(unavailable)?;
        if end <= now {
            expired.push((pair[0].as_str(), pair[1].as_str()));
        }
    }
    Ok(expired)
}

#[cfg(test)]
#[path = "../../tests/unit/redis_limiter/mod.rs"]
mod tests;
