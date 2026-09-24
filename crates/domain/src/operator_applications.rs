//! Complete application specifications for a single authenticated mutation.
use crate::{
    identity::ApplicationId,
    operator_accounts::{Error, checked_reason},
    registration::ApplicationSpec,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    Create(ApplicationSpec),
    Update {
        application: ApplicationId,
        revision: u64,
        spec: ApplicationSpec,
    },
}
#[derive(Debug)]
pub struct Request {
    operation: Operation,
    reason: String,
}
impl Request {
    pub fn new(operation: Operation, reason: &str) -> Result<Self, Error> {
        if matches!(operation, Operation::Update { revision, .. } if revision > i64::MAX as u64) {
            return Err(Error::Invalid);
        }
        Ok(Self {
            operation,
            reason: checked_reason(reason)?,
        })
    }
    pub fn operation(&self) -> &Operation {
        &self.operation
    }
    pub fn reason(&self) -> &str {
        &self.reason
    }
}
#[cfg(test)]
#[path = "../tests/unit/operator_applications.rs"]
mod tests;
