//! Shared attempt counters; only primary-authenticated identities select caller keys.
use crate::redis_limiter::RedisLimiter;
use darkhorse_application::{
    introspection_admission::{Budgets, Caller},
    limiting::{Admission, AttemptLimiter},
};
use darkhorse_domain::{
    limiting::{Attempt, Budget, BudgetRule},
    tokens::Error,
};
use envbind::{Binder, Environment, IntVar, ParameterSource};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    global: BudgetRule,
    caller: BudgetRule,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigurationError;
impl Policy {
    pub fn new(global: u32, caller: u32) -> Result<Self, ConfigurationError> {
        if caller > global {
            return Err(ConfigurationError);
        }
        Ok(Self {
            global: BudgetRule::new(global, 60_000)
                .map_err(|_| ConfigurationError)?
                .bound_to(caller),
            caller: BudgetRule::new(caller, 60_000).map_err(|_| ConfigurationError)?,
        })
    }
}
struct Raw {
    global: i64,
    caller: i64,
}
impl ParameterSource for Raw {
    fn bind<E: Environment>(b: &Binder<E>) -> Result<Self, envbind::BindError> {
        Ok(Self {
            global: b
                .bind(&IntVar::new("DARKHORSE_INTROSPECTION_GLOBAL_PER_MINUTE").default(60_000))?,
            caller: b
                .bind(&IntVar::new("DARKHORSE_INTROSPECTION_CALLER_PER_MINUTE").default(6_000))?,
        })
    }
}
pub fn load(environment: impl Environment) -> Result<Policy, ConfigurationError> {
    let raw = Raw::from_environment(environment).map_err(|_| ConfigurationError)?;
    Policy::new(
        raw.global.try_into().map_err(|_| ConfigurationError)?,
        raw.caller.try_into().map_err(|_| ConfigurationError)?,
    )
}
struct GlobalQueue {
    lanes: tokio::sync::Semaphore,
    waiters: tokio::sync::Semaphore,
}
impl Default for GlobalQueue {
    fn default() -> Self {
        Self {
            lanes: tokio::sync::Semaphore::new(2),
            waiters: tokio::sync::Semaphore::new(16),
        }
    }
}
impl GlobalQueue {
    fn enter(&self) -> Result<tokio::sync::SemaphorePermit<'_>, Error> {
        self.waiters.try_acquire().map_err(|_| Error::Unavailable)
    }
    async fn lock(
        &self,
    ) -> Result<
        (
            tokio::sync::SemaphorePermit<'_>,
            tokio::sync::SemaphorePermit<'_>,
        ),
        Error,
    > {
        let permit = self.enter()?;
        let guard = self.lanes.acquire().await.map_err(|_| Error::Unavailable)?;
        Ok((permit, guard))
    }
}
pub struct SharedBudgets {
    global_queue: GlobalQueue,
    limiter: RedisLimiter,
    key: Zeroizing<[u8; 32]>,
    policy: Policy,
}
impl SharedBudgets {
    pub fn new(limiter: RedisLimiter, key: [u8; 32], policy: Policy) -> Self {
        Self {
            global_queue: GlobalQueue::default(),
            limiter,
            key: Zeroizing::new(key),
            policy,
        }
    }
    async fn consume(&self, caller: Option<Caller>) -> Result<(), Error> {
        let attempt = attempt(&self.key, self.policy, caller)?;
        outcome(
            self.limiter
                .consume(&attempt)
                .await
                .map_err(|_| Error::Unavailable)?,
        )
    }
}
impl Budgets for SharedBudgets {
    async fn global(&self) -> Result<(), Error> {
        let started = std::time::Instant::now();
        let duration =
            std::time::Duration::from_millis(darkhorse_domain::limiter_recovery::OPERATION_MS);
        let result = tokio::time::timeout(duration, async {
            let _guard = self.global_queue.lock().await?;
            self.consume(None).await
        })
        .await
        .map_err(|_| Error::Unavailable)?;
        if started.elapsed() > duration {
            return Err(Error::Unavailable);
        }
        result
    }
    async fn caller(&self, caller: Caller) -> Result<(), Error> {
        self.consume(Some(caller)).await
    }
}
fn attempt(key: &[u8; 32], policy: Policy, caller: Option<Caller>) -> Result<Attempt, Error> {
    let (purpose, id, rule) = match caller {
        None => (b"introspection:v1:global".as_slice(), 0, policy.global),
        Some(Caller::Client(id)) => (
            b"introspection:v1:client".as_slice(),
            id.as_u128(),
            policy.caller,
        ),
        Some(Caller::Resource(id)) => (
            b"introspection:v1:resource".as_slice(),
            id.as_u128(),
            policy.caller,
        ),
    };
    let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| Error::Unavailable)?;
    mac.update(purpose);
    mac.update(&[0]);
    mac.update(&id.to_be_bytes());
    Attempt::new(vec![Budget {
        key: mac.finalize().into_bytes().into(),
        rule,
    }])
    .map_err(|_| Error::Unavailable)
}
fn outcome(admission: Admission) -> Result<(), Error> {
    match admission {
        Admission::Allowed => Ok(()),
        Admission::Limited { retry_after_ms } => Err(Error::Limited { retry_after_ms }),
    }
}
#[cfg(test)]
#[path = "../tests/unit/introspection_admission/mod.rs"]
mod tests;
