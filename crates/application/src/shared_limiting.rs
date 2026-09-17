use crate::limiting::{Admission, LimiterUnavailable};
use darkhorse_domain::{
    limiter_recovery::{Enforcement, Generation, ServerIdentity},
    limiting::{Attempt, Counter},
};
use std::future::Future;

pub trait EnforcementAuthority: Sync {
    fn read(&self) -> impl Future<Output = Result<Enforcement, LimiterUnavailable>> + Send;
}
pub trait RecoveryAuthority: EnforcementAuthority {
    fn fence(
        &self,
        nonce: [u8; 16],
    ) -> impl Future<Output = Result<Enforcement, LimiterUnavailable>> + Send;
    fn activate(
        &self,
        generation: Generation,
        identity: ServerIdentity,
    ) -> impl Future<Output = Result<(), LimiterUnavailable>> + Send;
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub counters: Vec<Option<Counter>>,
    pub redis_ms: u64,
    pub previous_ms: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Commit {
    Applied,
    Conflict,
    Full,
}
pub trait CounterStore: Sync {
    fn snapshot(
        &self,
        state: Enforcement,
        attempt: &Attempt,
    ) -> impl Future<Output = Result<Snapshot, LimiterUnavailable>> + Send;
    fn apply(
        &self,
        state: Enforcement,
        attempt: &Attempt,
        snapshot: &Snapshot,
        counters: &[Counter],
        now_ms: u64,
    ) -> impl Future<Output = Result<Commit, LimiterUnavailable>> + Send;
    fn prune(
        &self,
        state: Enforcement,
    ) -> impl Future<Output = Result<(), LimiterUnavailable>> + Send;
}

pub async fn consume(
    authority: &impl EnforcementAuthority,
    counters: &impl CounterStore,
    attempt: &Attempt,
) -> Result<Admission, LimiterUnavailable> {
    let state = authority.read().await?;
    darkhorse_domain::limiter_recovery::require_active(state).map_err(|_| LimiterUnavailable)?;
    for _ in 0..3 {
        let snapshot = counters.snapshot(state, attempt).await?;
        let (now, plan) = proposal(state, attempt, &snapshot)?;
        match plan {
            darkhorse_domain::limiting::Plan::Limited { retry_after_ms } => {
                return Ok(Admission::Limited { retry_after_ms });
            }
            darkhorse_domain::limiting::Plan::Charge(next) => match counters
                .apply(state, attempt, &snapshot, &next, now)
                .await?
            {
                Commit::Applied => {
                    darkhorse_domain::limiter_recovery::confirm(state, authority.read().await?)
                        .map_err(|_| LimiterUnavailable)?;
                    return Ok(Admission::Allowed);
                }
                Commit::Conflict => {}
                Commit::Full => counters.prune(state).await?,
            },
        }
    }
    Err(LimiterUnavailable)
}
fn proposal(
    state: Enforcement,
    attempt: &Attempt,
    snapshot: &Snapshot,
) -> Result<(u64, darkhorse_domain::limiting::Plan), LimiterUnavailable> {
    let now = darkhorse_domain::limiter_recovery::trusted_time(
        state.now_ms,
        snapshot.redis_ms,
        snapshot.previous_ms,
    )
    .map_err(|_| LimiterUnavailable)?;
    let plan = darkhorse_domain::limiting::plan(attempt, &snapshot.counters, now)
        .map_err(|_| LimiterUnavailable)?;
    Ok((now, plan))
}

#[cfg(test)]
#[path = "../tests/unit/shared_limiting.rs"]
mod tests;
