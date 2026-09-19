//! Proof of control of the current email. This never grants authority or resets credentials.
pub const LIFETIME_MS: u64 = 15 * 60 * 1000;
pub const DAY_MS: u64 = 24 * 60 * 60 * 1000;
pub const MAX_QUEUED: u64 = 10_000;
pub const LEASE_MS: u64 = 60_000;
pub const MAX_ATTEMPTS: u16 = 5;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Unauthorized,
    Throttled,
    Unavailable,
}
pub struct Proof<'a> {
    pub email: &'a str,
    pub epoch: u64,
    pub created_ms: u64,
    pub expires_ms: u64,
    pub consumed: bool,
}
pub fn request(last: Option<u64>, daily: u64, queued: u64, now: u64) -> Result<u64, Error> {
    if daily >= 5 || last.is_some_and(|last| now < last.saturating_add(LIFETIME_MS)) {
        return Err(Error::Throttled);
    }
    if queued >= MAX_QUEUED {
        return Err(Error::Unavailable);
    }
    now.checked_add(LIFETIME_MS)
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or(Error::Unavailable)
}
pub fn redeem(proof: &Proof<'_>, email: &str, epoch: u64, now: u64) -> Result<(), Error> {
    if proof.consumed
        || proof.email != email
        || proof.epoch != epoch
        || now < proof.created_ms
        || now >= proof.expires_ms
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
pub fn lease(now: u64, expires: u64) -> u64 {
    now.saturating_add(LEASE_MS).min(expires)
}
pub fn retry(attempt: u16, now: u64, expires: u64) -> Option<u64> {
    if !(1..MAX_ATTEMPTS).contains(&attempt) {
        return None;
    }
    now.checked_add(60_000 << (attempt - 1))
        .filter(|next| *next < expires)
}
#[cfg(test)]
#[path = "../tests/unit/email_verification.rs"]
mod tests;
