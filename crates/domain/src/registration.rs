//! Registration policy from explicit values and current authority facts.
use crate::{
    authentication::{SessionFacts, session_live},
    identity::*,
};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationError {
    Invalid,
    Unauthorized,
    Forbidden,
    RecentAuthenticationRequired,
    NotFound,
    Conflict,
    Unavailable,
}
pub const RECENT_AUTH_MS: u64 = 300_000;
pub const MAX_OVERLAP_SECONDS: u16 = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label(String);
impl Label {
    pub fn new(value: &str) -> Result<Self, RegistrationError> {
        let value = value.trim();
        if value.is_empty() || value.chars().count() > 100 || value.chars().any(char::is_control) {
            return Err(RegistrationError::Invalid);
        }
        Ok(Self(value.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeName(String);
impl ScopeName {
    pub fn new(value: &str) -> Result<Self, RegistrationError> {
        if value.is_empty()
            || value.len() > 100
            || !value
                .bytes()
                .all(|b| b == 0x21 || (0x23..=0x5b).contains(&b) || (0x5d..=0x7e).contains(&b))
        {
            return Err(RegistrationError::Invalid);
        }
        // Protocol scopes are assigned by the provider, not resource owners.
        if [
            "openid",
            "profile",
            "email",
            "address",
            "phone",
            "offline_access",
        ]
        .contains(&value)
        {
            return Err(RegistrationError::Invalid);
        }
        Ok(Self(value.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirects(Vec<String>);
impl Redirects {
    /// URL syntax/profile validation belongs to the transport adapter. This
    /// constructor enforces catalog bounds and exact identity, never normalization.
    pub fn from_validated_urls(values: Vec<String>) -> Result<Self, RegistrationError> {
        bounded_unique(&values, 8)?;
        if values.is_empty() || values.iter().any(|v| v.is_empty() || v.len() > 2048) {
            return Err(RegistrationError::Invalid);
        }
        Ok(Self(values))
    }
    pub fn values(&self) -> &[String] {
        &self.0
    }
    pub fn allows(&self, candidate: &str) -> bool {
        self.0.iter().any(|uri| uri == candidate)
    }
}
#[derive(Debug, Clone)]
pub struct ApplicationSpec {
    pub name: Label,
    pub owner: PrincipalId,
    pub active: bool,
}
#[derive(Debug, Clone)]
pub struct ClientSpec {
    pub name: Label,
    pub active: bool,
    pub redirects: Redirects,
    pub resources: Vec<ResourceId>,
    pub scopes: Vec<ScopeId>,
}
impl ClientSpec {
    pub fn new(
        name: Label,
        active: bool,
        redirects: Redirects,
        resources: Vec<ResourceId>,
        scopes: Vec<ScopeId>,
        method: &str,
    ) -> Result<Self, RegistrationError> {
        if method != "client_secret_basic" {
            return Err(RegistrationError::Invalid);
        }
        bounded_unique(&resources, 32)?;
        bounded_unique(&scopes, 128)?;
        Ok(Self {
            name,
            active,
            redirects,
            resources,
            scopes,
        })
    }
}
fn bounded_unique<T: Ord>(values: &[T], limit: usize) -> Result<(), RegistrationError> {
    if values.len() > limit || values.iter().collect::<BTreeSet<_>>().len() != values.len() {
        return Err(RegistrationError::Invalid);
    }
    Ok(())
}
pub fn administrator(
    session: SessionFacts,
    platform_admin: bool,
    now: u64,
    mutation: bool,
) -> Result<(), RegistrationError> {
    if !session_live(session, now) {
        return Err(RegistrationError::Unauthorized);
    }
    if !platform_admin {
        return Err(RegistrationError::Forbidden);
    }
    if mutation && now - session.created_ms >= RECENT_AUTH_MS {
        return Err(RegistrationError::RecentAuthenticationRequired);
    }
    Ok(())
}
pub fn next_revision(current: u64, expected: u64) -> Result<u64, RegistrationError> {
    if current != expected {
        return Err(RegistrationError::Conflict);
    }
    current
        .checked_add(1)
        .filter(|&v| v <= i64::MAX as u64)
        .ok_or(RegistrationError::Conflict)
}
pub fn overlap_deadline(now: u64, seconds: u16) -> Result<u64, RegistrationError> {
    if seconds > MAX_OVERLAP_SECONDS {
        return Err(RegistrationError::Invalid);
    }
    now.checked_add(u64::from(seconds) * 1000)
        .filter(|&v| v <= i64::MAX as u64)
        .ok_or(RegistrationError::Invalid)
}
pub fn secret_live(created: u64, expires: Option<u64>, retired: bool, now: u64) -> bool {
    !retired && now >= created && expires.is_none_or(|end| now < end)
}
pub fn grants_match(
    application: ApplicationId,
    client: &ClientSpec,
    resources: &[(ResourceId, ApplicationId)],
    scopes: &[(ScopeId, ResourceId, ApplicationId)],
) -> Result<(), RegistrationError> {
    if !client
        .resources
        .iter()
        .all(|id| resources.contains(&(*id, application)))
        || !client.scopes.iter().all(|id| {
            scopes.iter().any(|(scope, resource, app)| {
                id == scope && *app == application && client.resources.contains(resource)
            })
        })
    {
        return Err(RegistrationError::Invalid);
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/registration.rs"]
mod tests;
