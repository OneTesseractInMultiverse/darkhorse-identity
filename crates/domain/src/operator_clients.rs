//! A complete, scoped client replacement authorized by one password proof.
use crate::{
    identity::{ApplicationId, ClientId},
    operator_accounts::{Error, checked_reason},
    registration::ClientSpec,
};

#[derive(Debug, Clone)]
pub struct Update {
    pub application: ApplicationId,
    pub client: ClientId,
    pub revision: u64,
    pub spec: ClientSpec,
}
#[derive(Debug)]
pub struct Request {
    update: Update,
    reason: String,
}
impl Request {
    pub fn new(update: Update, reason: &str) -> Result<Self, Error> {
        if update.revision > i64::MAX as u64 {
            return Err(Error::Invalid);
        }
        let spec = &update.spec;
        ClientSpec::new(
            spec.name.clone(),
            spec.active,
            spec.redirects.clone(),
            spec.resources.clone(),
            spec.scopes.clone(),
            "client_secret_basic",
        )
        .map_err(|_| Error::Invalid)?;
        Ok(Self {
            update,
            reason: checked_reason(reason)?,
        })
    }
    pub fn update(&self) -> &Update {
        &self.update
    }
    pub fn reason(&self) -> &str {
        &self.reason
    }
}
#[cfg(test)]
#[path = "../tests/unit/operator_clients.rs"]
mod tests;
