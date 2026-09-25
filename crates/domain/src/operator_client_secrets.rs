//! Explicit, bounded credential metadata and terminal retirement operations.
use crate::{
    identity::{ApplicationId, ClientId, ClientSecretId},
    operator_accounts::{Error, checked_reason},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub application: ApplicationId,
    pub client: ClientId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    List {
        after: Option<ClientSecretId>,
        limit: u16,
    },
    Retire {
        secret: ClientSecretId,
        revision: u64,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    target: Target,
    operation: Operation,
    reason: Option<String>,
}
impl Request {
    pub fn new(target: Target, operation: Operation, reason: Option<&str>) -> Result<Self, Error> {
        let reason = match operation {
            Operation::List { limit, .. } => {
                if !(1..=25).contains(&limit) || reason.is_some() {
                    return Err(Error::Invalid);
                }
                None
            }
            Operation::Retire { revision, .. } => {
                if revision > i64::MAX as u64 {
                    return Err(Error::Invalid);
                }
                Some(checked_reason(reason.unwrap_or(""))?)
            }
        };
        Ok(Self {
            target,
            operation,
            reason,
        })
    }
    pub fn target(&self) -> Target {
        self.target
    }
    pub fn operation(&self) -> Operation {
        self.operation
    }
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}
#[cfg(test)]
#[path = "../tests/unit/operator_client_secrets.rs"]
mod tests;
