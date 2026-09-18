//! Pending authorization policy. Consent never confers resource permissions.
use crate::identity::{ClientId, PrincipalId};
use std::collections::BTreeSet;
pub const TRANSACTION_MS: u64 = 300_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidRequest,
    InvalidScope,
    InvalidTarget,
    InvalidTransaction,
    LoginRequired,
    ConsentRequired,
    AccessDenied,
    Unavailable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prompt {
    Default,
    None,
    Login,
    Consent,
    LoginConsent,
}
#[derive(Clone)]
pub struct Request {
    pub client: ClientId,
    pub redirect: String,
    pub challenge: [u8; 32],
    pub state: Option<String>,
    pub nonce: Option<String>,
    pub scopes: Vec<String>,
    pub resource: Option<String>,
    pub prompt: Prompt,
    pub max_age: Option<u64>,
}
impl Request {
    pub fn validate(&self) -> Result<(), Error> {
        if !text(&self.redirect, 2048)
            || !optional_text(self.state.as_deref(), 512)
            || !optional_text(self.nonce.as_deref(), 256)
            || !optional_text(self.resource.as_deref(), 256)
            || self.max_age.is_some_and(|age| age > 28_800)
            || self.scopes.is_empty()
            || self.scopes.len() > 32
            || self.scopes.iter().collect::<BTreeSet<_>>().len() != self.scopes.len()
            || self.scopes.iter().any(|s| !scope(s))
        {
            return Err(Error::InvalidRequest);
        }
        Ok(())
    }
}
fn optional_text(value: Option<&str>, limit: usize) -> bool {
    value.is_none_or(|s| text(s, limit))
}
fn text(s: &str, limit: usize) -> bool {
    !s.is_empty() && s.len() <= limit && s.bytes().all(|c| (0x20..=0x7e).contains(&c))
}
fn scope(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && s.bytes()
            .all(|b| b == 0x21 || (0x23..=0x5b).contains(&b) || (0x5d..=0x7e).contains(&b))
}
pub struct ClientPolicy {
    pub active: bool,
    pub revision: u64,
    pub application_revision: u64,
    pub redirects: Vec<String>,
    pub resources: Vec<(String, Vec<String>)>,
}
pub fn trusted_redirect(request: &Request, policy: &ClientPolicy) -> bool {
    policy.active && policy.redirects.contains(&request.redirect)
}
pub fn authorize_request(request: &Request, policy: &ClientPolicy) -> Result<(), Error> {
    request.validate()?;
    if !trusted_redirect(request, policy) {
        return Err(Error::InvalidRequest);
    }
    let resource = match request.resource.as_ref() {
        Some(audience) => Some(
            policy
                .resources
                .iter()
                .find(|(id, _)| id == audience)
                .ok_or(Error::InvalidTarget)?,
        ),
        None => None,
    };
    if !request.scopes.iter().any(|scope| scope == "openid")
        || request.scopes.iter().any(|scope| {
            scope != "openid" && resource.is_none_or(|(_, scopes)| !scopes.contains(scope))
        })
    {
        return Err(Error::InvalidScope);
    }
    Ok(())
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Session {
    pub digest: [u8; 32],
    pub principal: PrincipalId,
    pub authenticated_ms: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interaction {
    Login,
    Consent,
    Ready,
}
pub fn interaction(
    request: &Request,
    created: u64,
    now: u64,
    initial: Option<[u8; 32]>,
    bound: Option<Session>,
    current: Option<Session>,
    consented: bool,
) -> Result<Interaction, Error> {
    if now < created
        || now - created >= TRANSACTION_MS
        || current.is_some_and(|s| s.authenticated_ms > now)
    {
        return Err(Error::InvalidTransaction);
    }
    if bound.is_some() && bound != current {
        return Err(Error::InvalidTransaction);
    }
    if needs_login(request, created, now, initial, current) {
        return if request.prompt == Prompt::None {
            Err(Error::LoginRequired)
        } else {
            Ok(Interaction::Login)
        };
    }
    if !consented || matches!(request.prompt, Prompt::Consent | Prompt::LoginConsent) {
        return if request.prompt == Prompt::None {
            Err(Error::ConsentRequired)
        } else {
            Ok(Interaction::Consent)
        };
    }
    Ok(Interaction::Ready)
}
fn needs_login(
    r: &Request,
    created: u64,
    now: u64,
    initial: Option<[u8; 32]>,
    current: Option<Session>,
) -> bool {
    let Some(s) = current else { return true };
    let forced = matches!(r.prompt, Prompt::Login | Prompt::LoginConsent) || r.max_age == Some(0);
    (forced && (initial == Some(s.digest) || s.authenticated_ms < created))
        || r.max_age
            .is_some_and(|age| age > 0 && now - s.authenticated_ms > age * 1000)
}
pub fn continued_prompt(prompt: Prompt, approved: bool) -> Prompt {
    if !approved {
        return prompt;
    }
    match prompt {
        Prompt::Consent => Prompt::Default,
        Prompt::LoginConsent => Prompt::Login,
        _ => prompt,
    }
}
#[cfg(test)]
#[path = "../tests/unit/oidc.rs"]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Inspect,
    Approve,
    Deny,
}
pub fn decision(stage: Interaction, approved: bool, decision: Decision) -> Result<Decision, Error> {
    if decision == Decision::Approve {
        if approved {
            return Err(Error::InvalidTransaction);
        }
        if stage == Interaction::Login {
            return Err(Error::LoginRequired);
        }
    }
    Ok(decision)
}
pub fn capacity(total: u64, client: u64) -> Result<(), Error> {
    if total >= 5000 || client >= 100 {
        return Err(Error::Unavailable);
    }
    Ok(())
}
