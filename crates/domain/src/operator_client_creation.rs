//! One authenticated client creation request for the native operator interface.
use crate::{
    identity::ApplicationId,
    operator_accounts::{Error, checked_reason},
    registration::ClientSpec,
};

#[derive(Debug)]
pub struct Request {
    application: ApplicationId,
    spec: ClientSpec,
    reason: String,
}

impl Request {
    pub fn new(application: ApplicationId, spec: ClientSpec, reason: &str) -> Result<Self, Error> {
        Ok(Self {
            application,
            spec,
            reason: checked_reason(reason)?,
        })
    }

    pub fn application(&self) -> ApplicationId {
        self.application
    }

    pub fn spec(&self) -> &ClientSpec {
        &self.spec
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

#[cfg(test)]
#[path = "../tests/unit/operator_client_creation.rs"]
mod tests;
