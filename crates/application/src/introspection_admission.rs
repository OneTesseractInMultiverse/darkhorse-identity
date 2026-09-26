//! Admission authenticates a caller without carrying authority into token evaluation.
use darkhorse_domain::{
    identity::{ClientId, ResourceId},
    tokens::Error,
};
use std::future::Future;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Caller {
    Client(ClientId),
    Resource(ResourceId),
}
// Credential material deliberately has no Debug implementation.
pub struct Credentials {
    pub caller: Caller,
    pub secret: [u8; 32],
}
pub trait Authenticate: Send + Sync {
    fn authenticate_introspection(
        &self,
        credentials: Credentials,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
pub trait Budgets: Send + Sync {
    fn global(&self) -> impl Future<Output = Result<(), Error>> + Send;
    fn caller(&self, caller: Caller) -> impl Future<Output = Result<(), Error>> + Send;
}
pub trait Gate: Send + Sync {
    fn before(&self) -> impl Future<Output = Result<(), Error>> + Send;
    fn authenticated(
        &self,
        credentials: Credentials,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
pub struct Service<S, L> {
    pub store: S,
    pub budgets: L,
}
impl<S: Authenticate, L: Budgets> Gate for Service<S, L> {
    async fn before(&self) -> Result<(), Error> {
        self.budgets.global().await
    }
    async fn authenticated(&self, credentials: Credentials) -> Result<(), Error> {
        let caller = credentials.caller;
        self.store.authenticate_introspection(credentials).await?;
        self.budgets.caller(caller).await
    }
}
#[cfg(test)]
#[path = "../tests/unit/introspection_admission.rs"]
mod tests;
