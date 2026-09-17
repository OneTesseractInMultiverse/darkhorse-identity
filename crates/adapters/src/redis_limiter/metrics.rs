use darkhorse_application::limiting::{Admission, LimiterUnavailable};
use std::sync::atomic::{AtomicU64, Ordering};
#[derive(Default)]
pub(super) struct Counters {
    allowed: AtomicU64,
    limited: AtomicU64,
    unavailable: AtomicU64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcomes {
    pub allowed: u64,
    pub limited: u64,
    pub unavailable: u64,
}
impl Counters {
    pub(super) fn record(&self, result: &Result<Admission, LimiterUnavailable>) {
        self.counter(result).fetch_add(1, Ordering::Relaxed);
    }
    fn counter(&self, result: &Result<Admission, LimiterUnavailable>) -> &AtomicU64 {
        match result {
            Ok(Admission::Allowed) => &self.allowed,
            Ok(Admission::Limited { .. }) => &self.limited,
            Err(_) => &self.unavailable,
        }
    }
    pub(super) fn snapshot(&self) -> Outcomes {
        Outcomes {
            allowed: self.allowed.load(Ordering::Relaxed),
            limited: self.limited.load(Ordering::Relaxed),
            unavailable: self.unavailable.load(Ordering::Relaxed),
        }
    }
}
#[cfg(test)]
#[path = "../../tests/unit/redis_limiter/metrics.rs"]
mod tests;
