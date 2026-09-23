//! Durable intent precedes the external effect; its receipt commits with activation.
use darkhorse_domain::{
    identity::OperationId,
    limiter_recovery::{Enforcement, Generation, ServerIdentity, activation_ready},
};
use std::future::Future;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    NotReady,
    NotFound,
    Unavailable,
    Uncertain,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub completed_ms: u64,
    pub identity: ServerIdentity,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    pub id: OperationId,
    pub generation: Generation,
    pub not_before_ms: u64,
    pub prepared_ms: u64,
    pub database_role: String,
    pub completion: Option<Completion>,
    pub current: Enforcement,
}
pub trait Journal: Sync {
    fn prepare(&self, id: OperationId) -> impl Future<Output = Result<Enforcement, Error>> + Send;
    fn complete(
        &self,
        id: OperationId,
        generation: Generation,
        identity: ServerIdentity,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn inspect(&self, id: OperationId) -> impl Future<Output = Result<Attempt, Error>> + Send;
}
pub trait Initializer: Sync {
    fn initialize(
        &self,
        state: Enforcement,
    ) -> impl Future<Output = Result<ServerIdentity, Error>> + Send;
}
pub async fn activate(
    journal: &impl Journal,
    counters: &impl Initializer,
    id: OperationId,
) -> Result<(), Error> {
    let state = journal.prepare(id).await?;
    activation_ready(state).map_err(|_| Error::NotReady)?;
    let identity = counters
        .initialize(state)
        .await
        .map_err(|_| Error::Uncertain)?;
    journal
        .complete(id, state.generation, identity)
        .await
        .map_err(|_| Error::Uncertain)
}
#[cfg(test)]
#[path = "../tests/unit/limiter_activation.rs"]
mod tests;
