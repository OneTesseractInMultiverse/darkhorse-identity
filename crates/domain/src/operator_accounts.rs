//! Policy for a single authenticated local account operation.
use crate::{AccountStatus, directory::AccountAction, identity::PrincipalId};

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
/// Success and target-dependent failures require authority through completion.
pub fn needs_current_authority<T>(outcome: &Result<T, Error>) -> bool {
    match outcome {
        Ok(_) | Err(Error::Invalid | Error::NotFound | Error::Conflict | Error::PolicyRejected) => {
            true
        }
        Err(Error::Denied | Error::Limited { .. } | Error::Unavailable | Error::Uncertain) => false,
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActorState {
    pub status: AccountStatus,
    pub epoch: u64,
}
/// Derive the sole allowed actor transition from the authenticated request.
/// Credential identity, verifier, revocation, membership and proof age still apply.
pub fn completion_state(
    actor: PrincipalId,
    epoch: u64,
    operation: Operation,
    changed: bool,
) -> Result<ActorState, Error> {
    let original = ActorState {
        status: AccountStatus::Active,
        epoch,
    };
    let Operation::Change { target, action, .. } = operation else {
        return Ok(original);
    };
    if !changed || target != actor {
        return Ok(original);
    }
    let epoch = epoch
        .checked_add(1)
        .filter(|e| *e <= i64::MAX as u64)
        .ok_or(Error::Denied)?;
    match action {
        AccountAction::RevokeAll => Ok(ActorState { epoch, ..original }),
        AccountAction::SetStatus(AccountStatus::Inactive) => Ok(ActorState {
            status: AccountStatus::Inactive,
            epoch,
        }),
        // A verified actor started active; an actual self-reactivation is unexpected.
        AccountAction::SetStatus(AccountStatus::Active) => Err(Error::Denied),
    }
}
pub(crate) fn checked_reason(value: &str) -> Result<String, Error> {
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
