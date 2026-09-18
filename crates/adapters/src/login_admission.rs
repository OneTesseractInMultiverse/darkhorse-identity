//! Fixed server-owned budgets; keyed identities prevent disclosure in Redis.
use crate::redis_limiter::RedisLimiter;
use darkhorse_application::{
    authentication::{AuthError, LoginAdmission},
    limiting::{Admission, AttemptLimiter},
};
use darkhorse_domain::limiting::{Attempt, Budget, BudgetRule};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use zeroize::Zeroizing;

pub struct SharedLoginAdmission {
    limiter: RedisLimiter,
    key: Zeroizing<[u8; 32]>,
}
impl SharedLoginAdmission {
    pub fn new(limiter: RedisLimiter, key: [u8; 32]) -> Self {
        Self {
            limiter,
            key: Zeroizing::new(key),
        }
    }
}
impl LoginAdmission for SharedLoginAdmission {
    async fn admit(&self, email_key: &str) -> Result<(), AuthError> {
        let attempt = attempt(&self.key, email_key)?;
        outcome(
            self.limiter
                .consume(&attempt)
                .await
                .map_err(|_| AuthError::Unavailable)?,
        )
    }
}
fn outcome(admission: Admission) -> Result<(), AuthError> {
    match admission {
        Admission::Allowed => Ok(()),
        Admission::Limited { retry_after_ms } => Err(AuthError::Limited { retry_after_ms }),
    }
}
fn attempt(key: &[u8; 32], email: &str) -> Result<Attempt, AuthError> {
    Attempt::new(vec![
        budget(key, b"login:v1:global", b"", 120, 60_000)?,
        budget(key, b"login:v1:account:minute", email.as_bytes(), 5, 60_000)?,
        budget(
            key,
            b"login:v1:account:quarter",
            email.as_bytes(),
            30,
            900_000,
        )?,
    ])
    .map_err(|_| AuthError::Unavailable)
}
fn budget(
    key: &[u8; 32],
    domain: &[u8],
    identity: &[u8],
    limit: u32,
    window: u32,
) -> Result<Budget, AuthError> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| AuthError::Unavailable)?;
    mac.update(domain);
    mac.update(&[0]);
    mac.update(identity);
    Ok(Budget {
        key: mac.finalize().into_bytes().into(),
        rule: BudgetRule::new(limit, window).map_err(|_| AuthError::Unavailable)?,
    })
}
#[cfg(test)]
#[path = "../tests/unit/login_admission.rs"]
mod tests;
