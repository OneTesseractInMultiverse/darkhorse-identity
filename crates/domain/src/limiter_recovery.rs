//! Durable fencing and explicit clock rules for shared admission recovery.
use crate::limiting::{LimitError, MAX_TIME, MAX_WINDOW_MS};

pub const OPERATION_MS: u64 = 1000;
pub const CLOCK_SKEW_MS: u64 = 1000;
pub const RECOVERY_WAIT_MS: u64 = MAX_WINDOW_MS as u64 + 2 * OPERATION_MS + 2 * CLOCK_SKEW_MS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Generation {
    epoch: u64,
    nonce: [u8; 16],
}
impl Generation {
    pub fn new(epoch: u64, nonce: [u8; 16]) -> Result<Self, LimitError> {
        if epoch == 0 || epoch > MAX_TIME || nonce == [0; 16] {
            return Err(LimitError::InvalidInput);
        }
        Ok(Self { epoch, nonce })
    }
    pub fn epoch(self) -> u64 {
        self.epoch
    }
    pub fn nonce(self) -> [u8; 16] {
        self.nonce
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerIdentity {
    pub run: [u8; 20],
    pub replication: [u8; 20],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Enforcement {
    pub generation: Generation,
    pub active: bool,
    pub not_before_ms: u64,
    pub now_ms: u64,
    pub identity: Option<ServerIdentity>,
}
pub fn recovery_deadline(now_ms: u64) -> Result<u64, LimitError> {
    now_ms
        .checked_add(RECOVERY_WAIT_MS)
        .filter(|&n| n <= MAX_TIME)
        .ok_or(LimitError::InvalidInput)
}
pub fn activation_ready(state: Enforcement) -> Result<(), LimitError> {
    if state.active || state.now_ms < state.not_before_ms || state.now_ms > MAX_TIME {
        return Err(LimitError::UnsafeState);
    }
    Ok(())
}
pub fn require_active(state: Enforcement) -> Result<ServerIdentity, LimitError> {
    if !state.active || state.now_ms < state.not_before_ms || state.now_ms > MAX_TIME {
        return Err(LimitError::UnsafeState);
    }
    state.identity.ok_or(LimitError::UnsafeState)
}
pub fn confirm(before: Enforcement, after: Enforcement) -> Result<(), LimitError> {
    if require_active(before)? != require_active(after)?
        || before.generation != after.generation
        || after.now_ms < before.now_ms
        || after.now_ms - before.now_ms > OPERATION_MS
    {
        return Err(LimitError::UnsafeState);
    }
    Ok(())
}
pub fn trusted_time(database_ms: u64, redis_ms: u64, previous_ms: u64) -> Result<u64, LimitError> {
    if database_ms > MAX_TIME
        || redis_ms > MAX_TIME
        || database_ms.abs_diff(redis_ms) > CLOCK_SKEW_MS
    {
        return Err(LimitError::UnsafeState);
    }
    let now = redis_ms;
    if now < previous_ms {
        return Err(LimitError::ClockRollback);
    }
    Ok(now)
}

#[cfg(test)]
#[path = "../tests/unit/limiter_recovery.rs"]
mod tests;
