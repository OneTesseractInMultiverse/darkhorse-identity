//! Shared admission enforcement. Only a known successful charge permits work.
use darkhorse_domain::limiting::Attempt;
use std::future::Future;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Allowed,
    Limited { retry_after_ms: u32 },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimiterUnavailable;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionFailure {
    Limited { retry_after_ms: u32 },
    Unavailable,
}

pub trait AttemptLimiter {
    /// Charge all requested budgets atomically, or charge none when limited.
    /// Missing/uncertain state is unavailable, never a new allowance. An
    /// ambiguous transport result must not be transparently retried.
    fn consume(
        &self,
        attempt: &Attempt,
    ) -> impl Future<Output = Result<Admission, LimiterUnavailable>> + Send;
}

/// One successful call permits one immediate attempt. It is not a reusable token.
pub async fn require_admission(
    limiter: &impl AttemptLimiter,
    attempt: &Attempt,
) -> Result<(), AdmissionFailure> {
    admission_result(limiter.consume(attempt).await)
}
fn admission_result(result: Result<Admission, LimiterUnavailable>) -> Result<(), AdmissionFailure> {
    match result {
        Ok(Admission::Allowed) => Ok(()),
        Ok(Admission::Limited { retry_after_ms }) => {
            Err(AdmissionFailure::Limited { retry_after_ms })
        }
        Err(LimiterUnavailable) => Err(AdmissionFailure::Unavailable),
    }
}
#[cfg(test)]
#[path = "../tests/unit/limiting.rs"]
mod tests;
