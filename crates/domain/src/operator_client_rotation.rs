//! One authenticated, revision-fenced operator client-secret rotation request.
use crate::{
    operator_accounts::{Error, checked_reason},
    operator_client_secrets::Target,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    target: Target,
    revision: u64,
    overlap_seconds: u16,
    reason: String,
}

impl Request {
    pub fn new(
        target: Target,
        revision: u64,
        overlap_seconds: u16,
        reason: &str,
    ) -> Result<Self, Error> {
        if revision >= i64::MAX as u64 || overlap_seconds > 300 {
            return Err(Error::Invalid);
        }
        Ok(Self {
            target,
            revision,
            overlap_seconds,
            reason: checked_reason(reason)?,
        })
    }

    pub fn target(&self) -> Target {
        self.target
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn overlap_seconds(&self) -> u16 {
        self.overlap_seconds
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

#[cfg(test)]
#[path = "../tests/unit/operator_client_rotation.rs"]
mod tests;
