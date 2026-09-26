//! One-time code and purpose-bound protocol credential policy.
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
    InvalidTarget,
    InvalidScope,
    InvalidToken,
    UnsupportedGrant,
    Limited { retry_after_ms: u32 },
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
    if resource.is_some() {
        return resource_profile(scopes);
    }
    if !scopes.iter().any(|s| s == "openid")
        || scopes.iter().any(|s| !identity_scope(s))
        || scopes
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != scopes.len()
    {
        return Err(Error::InvalidScope);
    }
    Ok(())
}
fn resource_profile(scopes: &[String]) -> Result<(), Error> {
    if !(2..=32).contains(&scopes.len())
        || !scopes.iter().any(|s| s == "openid")
        || scopes.iter().any(|s| {
            !crate::oidc::scope(s)
                || matches!(
                    s.as_str(),
                    "profile" | "email" | "address" | "phone" | "offline_access"
                )
        })
        || scopes
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != scopes.len()
    {
        return Err(Error::InvalidScope);
    }
    Ok(())
}
pub fn resource_scope_text(scopes: &[String]) -> Result<String, Error> {
    resource_profile(scopes)?;
    let mut sorted = scopes.to_vec();
    sorted.sort();
    Ok(sorted.join(" "))
}
pub fn resource_binding(granted: Option<&str>, requested: Option<&str>) -> Result<(), Error> {
    if requested.is_some() && requested != granted {
        return Err(Error::InvalidTarget);
    }
    Ok(())
}
pub fn identity_scope(scope: &str) -> bool {
    matches!(scope, "openid" | "profile" | "email")
}

pub fn scope_text(scopes: &[String]) -> Result<String, Error> {
    profile(scopes, None)?;
    Ok(["openid", "profile", "email"]
        .into_iter()
        .filter(|scope| scopes.iter().any(|s| s == scope))
        .collect::<Vec<_>>()
        .join(" "))
}
pub fn claim_ceiling(scopes: &[String]) -> Result<Vec<String>, Error> {
    profile(scopes, None)?;
    let mut claims = vec!["sub".into()];
    if scopes.iter().any(|s| s == "profile") {
        claims.extend(["name", "given_name", "family_name"].map(String::from));
    }
    if scopes.iter().any(|s| s == "email") {
        claims.extend(["email", "email_verified"].map(String::from));
    }
    Ok(claims)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Disclosure {
    pub profile: bool,
    pub email: bool,
}
pub fn disclosure(scopes: &[String], ceiling: &[String]) -> Result<Disclosure, Error> {
    if claim_ceiling(scopes)? != ceiling {
        return Err(Error::Unavailable);
    }
    Ok(Disclosure {
        profile: scopes.iter().any(|s| s == "profile"),
        email: scopes.iter().any(|s| s == "email"),
    })
}
pub fn consent(requested: &[String], approved: Option<&[String]>) -> Result<(), Error> {
    if approved.is_none_or(|allowed| requested.iter().any(|scope| !allowed.contains(scope))) {
        return Err(Error::InvalidGrant);
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
