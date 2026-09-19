//! Invitation-only onboarding creates an ordinary account, never an access grant.
pub const LIFETIME_MS: u64 = 86_400_000;
pub const COOLDOWN_MS: u64 = 900_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Unauthorized,
    Forbidden,
    RecentAuthentication,
    Conflict,
    Throttled,
    Unavailable,
}
pub fn email(value: &str) -> Result<String, Error> {
    crate::directory::login_email_key(value).map_err(|_| Error::Invalid)?;
    Ok(value.trim().to_owned())
}
pub fn issue(
    last: Option<u64>,
    recipient_daily: u64,
    actor_daily: u64,
    queued: u64,
    now: u64,
) -> Result<u64, Error> {
    if recipient_daily >= 5
        || actor_daily >= 100
        || last.is_some_and(|last| now < last.saturating_add(COOLDOWN_MS))
    {
        return Err(Error::Throttled);
    }
    if queued >= 10_000 {
        return Err(Error::Unavailable);
    }
    now.checked_add(LIFETIME_MS)
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or(Error::Unavailable)
}
pub fn ensure_new_recipient(exists: bool) -> Result<(), Error> {
    if exists { Err(Error::Conflict) } else { Ok(()) }
}
pub struct Proof {
    pub created_ms: u64,
    pub expires_ms: u64,
    pub closed: bool,
    pub issuer_eligible: bool,
    pub account_exists: bool,
}
pub fn redeem(proof: &Proof, now: u64) -> Result<(), Error> {
    if proof.closed
        || !proof.issuer_eligible
        || proof.account_exists
        || now < proof.created_ms
        || now >= proof.expires_ms
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
pub fn admit(attempts: u64, global_minute: u64) -> Result<(), Error> {
    if attempts >= 5 || global_minute >= 60 {
        return Err(Error::Throttled);
    }
    Ok(())
}
#[cfg(test)]
#[path = "../tests/unit/invitations.rs"]
mod tests;
