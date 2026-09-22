//! Policy for a single authenticated local account operation.
use crate::{directory::AccountAction, identity::PrincipalId};

pub const PROOF_LIFETIME_MS: u64 = 60_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Denied,
    Limited { retry_after_ms: u32 },
    NotFound,
    Conflict,
    PolicyRejected,
    Unavailable,
    Uncertain,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Show(PrincipalId),
    Change {
        target: PrincipalId,
        revision: u64,
        action: AccountAction,
    },
}
#[derive(Debug)]
pub struct Request {
    operation: Operation,
    reason: Option<String>,
}
impl Request {
    pub fn new(operation: Operation, reason: Option<&str>) -> Result<Self, Error> {
        let reason = reason.map(checked_reason).transpose()?;
        if let Operation::Change { revision, .. } = operation
            && (revision > i64::MAX as u64 || reason.is_none())
        {
            return Err(Error::Invalid);
        }
        Ok(Self { operation, reason })
    }
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
    pub fn operation(&self) -> Operation {
        self.operation
    }
}
#[derive(Clone, Copy)]
pub struct Authority {
    pub credential_current: bool,
    pub administrator: bool,
    pub observed_ms: u64,
}
pub fn authorize(authority: Authority, now: u64) -> Result<(), Error> {
    if !authority.credential_current
        || !authority.administrator
        || now
            .checked_sub(authority.observed_ms)
            .is_none_or(|age| age >= PROOF_LIFETIME_MS)
    {
        return Err(Error::Denied);
    }
    Ok(())
}
fn checked_reason(value: &str) -> Result<String, Error> {
    let value = value.trim();
    if value.is_empty() || value.len() > 512 || value.chars().count() > 200
        || value.chars().any(|c| c.is_control() || matches!(c,'\u{2028}'..='\u{202e}'|'\u{2066}'..='\u{2069}'|'\u{061c}'|'\u{200e}'|'\u{200f}')) {
        return Err(Error::Invalid);
    }
    Ok(value.to_owned())
}

#[cfg(test)]
#[path = "../tests/unit/operator_accounts.rs"]
mod tests;
