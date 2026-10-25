//! Bounded notification delivery policy, independent of clocks and transports.
pub const MAX_ATTEMPTS: u16 = 8;
pub const MAX_AGE_MS: u64 = 86_400_000;
pub const LEASE_MS: u64 = 30_000;
pub const RETENTION_MS: u64 = 604_800_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryResult {
    Accepted,
    Retry,
    Rejected,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Delivered,
    Failed,
    RetryAt(u64),
}

/// The next attempt number and lease deadline, or terminal exhaustion/expiry.
pub fn claim(attempts: u16, created: u64, now: u64) -> Option<(u16, u64)> {
    let deadline = created.checked_add(MAX_AGE_MS)?;
    if attempts >= MAX_ATTEMPTS || now < created || now >= deadline {
        return None;
    }
    Some((attempts + 1, now.checked_add(LEASE_MS)?))
}

pub fn outcome(result: DeliveryResult, attempt: u16, created: u64, now: u64) -> Outcome {
    let Some(deadline) = created.checked_add(MAX_AGE_MS) else {
        return Outcome::Failed;
    };
    if attempt == 0 || attempt > MAX_ATTEMPTS || now < created || now >= deadline {
        return Outcome::Failed;
    }
    match result {
        DeliveryResult::Accepted => Outcome::Delivered,
        DeliveryResult::Rejected => Outcome::Failed,
        DeliveryResult::Retry => retry(attempt, now, deadline),
    }
}
fn retry(attempt: u16, now: u64, deadline: u64) -> Outcome {
    let delays = [
        5_000, 30_000, 120_000, 600_000, 1_800_000, 3_600_000, 7_200_000,
    ];
    match delays
        .get(usize::from(attempt - 1))
        .and_then(|delay| now.checked_add(*delay))
    {
        Some(next) if next < deadline => Outcome::RetryAt(next),
        _ => Outcome::Failed,
    }
}

#[cfg(test)]
#[path = "../tests/unit/logout.rs"]
mod tests;
