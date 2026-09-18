//! One-time code and subject-only protocol credential policy.
use crate::{
    identity::{ClientId, PrincipalId},
    oidc::{ClientPolicy, Session},
};
pub const CODE_MS: u64 = 60_000;
pub const ACCESS_MS: u64 = 300_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidRequest,
    InvalidClient,
    InvalidGrant,
    InvalidScope,
    InvalidToken,
    UnsupportedGrant,
    Unavailable,
}
pub struct CodeFacts<'a> {
    pub client: ClientId,
    pub redirect: &'a str,
    pub challenge: [u8; 32],
    pub created_ms: u64,
    pub expires_ms: u64,
}
pub struct Proof<'a> {
    pub client: ClientId,
    pub redirect: &'a str,
    pub challenge: [u8; 32],
}
pub fn binding(code: &CodeFacts<'_>, proof: &Proof<'_>) -> Result<(), Error> {
    if code.client != proof.client
        || code.redirect != proof.redirect
        || code.challenge != proof.challenge
    {
        return Err(Error::InvalidGrant);
    }
    Ok(())
}
pub fn exchange(code: &CodeFacts<'_>, proof: &Proof<'_>, now: u64) -> Result<(), Error> {
    binding(code, proof)?;
    if now < code.created_ms
        || now >= code.expires_ms
        || code.expires_ms - code.created_ms > CODE_MS
    {
        return Err(Error::InvalidGrant);
    }
    Ok(())
}
pub fn deadline(now: u64, lifetime: u64) -> Result<u64, Error> {
    now.checked_add(lifetime)
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or(Error::Unavailable)
}
pub fn profile(scopes: &[String], resource: Option<&str>) -> Result<(), Error> {
    if scopes != ["openid"] || resource.is_some() {
        return Err(Error::InvalidScope);
    }
    Ok(())
}
pub fn live(created: u64, expires: u64, now: u64) -> bool {
    expires > created && expires - created <= ACCESS_MS && now >= created && now < expires
}
pub struct Grant {
    pub client_revision: u64,
    pub application_revision: u64,
    pub principal: PrincipalId,
    pub authenticated_ms: u64,
}
pub fn current(
    grant: &Grant,
    policy: &ClientPolicy,
    session: Option<Session>,
    redirect: &str,
) -> Result<(), Error> {
    if !policy.active
        || policy.revision != grant.client_revision
        || policy.application_revision != grant.application_revision
        || !policy.redirects.iter().any(|r| r == redirect)
    {
        return Err(Error::InvalidGrant);
    }
    let session = session.ok_or(Error::InvalidGrant)?;
    if session.principal != grant.principal || session.authenticated_ms != grant.authenticated_ms {
        return Err(Error::InvalidGrant);
    }
    Ok(())
}
#[cfg(test)]
#[path = "../tests/unit/tokens.rs"]
mod tests;
