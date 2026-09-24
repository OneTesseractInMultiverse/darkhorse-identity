//! Correlated signing changes with durable intent and atomic completion.
use crate::signing::WrappedKey;
use darkhorse_domain::{
    identity::OperationId,
    signing::{KeyError, Phase},
};
use std::future::Future;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Generate,
    Import,
    Activate,
    Retire,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Intent {
    pub id: OperationId,
    pub issuer: String,
    pub kid: String,
    pub kind: Kind,
    pub expected_revision: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Rejected(KeyError),
    Uncertain,
}
impl From<KeyError> for Error {
    fn from(value: KeyError) -> Self {
        Self::Rejected(value)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub revision: u64,
    pub completed_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    pub intent: Intent,
    pub database_role: String,
    pub prepared_ms: u64,
    pub completion: Option<Completion>,
    pub current_revision: Option<u64>,
    pub current_phase: Option<Phase>,
    pub database_ms: u64,
}
pub trait Journal: Sync {
    fn prepare_signing(&self, intent: &Intent) -> impl Future<Output = Result<(), Error>> + Send;
    fn complete_signing(
        &self,
        intent: &Intent,
        wrap_digest: [u8; 32],
        key: Option<WrappedKey>,
    ) -> impl Future<Output = Result<u64, Error>> + Send;
    fn inspect_signing(
        &self,
        id: OperationId,
    ) -> impl Future<Output = Result<Attempt, Error>> + Send;
}
pub async fn execute(
    journal: &impl Journal,
    intent: &Intent,
    wrap_digest: [u8; 32],
    key: Option<WrappedKey>,
) -> Result<u64, Error> {
    journal.prepare_signing(intent).await?;
    journal.complete_signing(intent, wrap_digest, key).await
}
#[cfg(test)]
#[path = "../tests/unit/signing_operations.rs"]
mod tests;
