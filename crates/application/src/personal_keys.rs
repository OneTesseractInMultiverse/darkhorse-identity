//! Personal credential lifecycle ports. Raw secrets never enter the store port.
use darkhorse_domain::{
    authorization::CapabilitySet,
    identity::*,
    personal_keys::{Error, ExpiryPolicy, Request},
};
use std::future::Future;

#[derive(Debug, Clone)]
pub struct Grant {
    pub resource: ResourceId,
    pub ceiling: CapabilitySet,
}
#[derive(Debug, Clone)]
pub struct Record {
    pub id: CredentialId,
    pub name: String,
    pub application: ApplicationId,
    pub created_ms: u64,
    pub expires_ms: Option<u64>,
    pub active: bool,
    pub grants: Vec<Grant>,
}
#[derive(Debug)]
pub struct Page {
    pub items: Vec<Record>,
    pub next: Option<CredentialId>,
}
#[derive(Debug)]
pub struct Eligible {
    pub application: ApplicationId,
    pub application_name: String,
    pub resource: ResourceId,
    pub resource_name: String,
    pub capabilities: Vec<Capability>,
}
#[derive(Debug)]
pub struct Capability {
    pub id: CapabilityId,
    pub key: String,
    pub meaning: String,
}
#[derive(Debug)]
pub struct Options {
    pub policy: ExpiryPolicy,
    pub revision: u64,
    pub items: Vec<Eligible>,
    pub next: Option<ResourceId>,
}
// Deliberately no Debug for credential material or its delivery response.
pub struct Verifier {
    pub id: CredentialId,
    pub digest: [u8; 32],
}
pub struct Prepared {
    pub value: String,
    pub verifier: Verifier,
}
pub struct Created {
    pub record: Record,
    pub secret: String,
}
pub trait Entropy: Send + Sync {
    fn key(&self) -> Result<Prepared, Error>;
}
pub trait Store: Send + Sync {
    fn options(
        &self,
        actor: [u8; 32],
        after: Option<ResourceId>,
    ) -> impl Future<Output = Result<Options, Error>> + Send;
    fn list(
        &self,
        actor: [u8; 32],
        after: Option<CredentialId>,
    ) -> impl Future<Output = Result<Page, Error>> + Send;
    fn preflight(
        &self,
        actor: [u8; 32],
        request: &Request,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn issue(
        &self,
        actor: [u8; 32],
        request: &Request,
        verifier: Verifier,
    ) -> impl Future<Output = Result<Record, Error>> + Send;
    fn revoke(
        &self,
        actor: [u8; 32],
        key: CredentialId,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
pub struct Service<S, E> {
    pub store: S,
    pub entropy: E,
}
impl<S: Store, E: Entropy> Service<S, E> {
    pub async fn create(&self, actor: [u8; 32], request: &Request) -> Result<Created, Error> {
        self.store.preflight(actor, request).await?;
        let Prepared { value, verifier } = self.entropy.key()?;
        let record = self.store.issue(actor, request, verifier).await?;
        Ok(Created {
            record,
            secret: value,
        })
    }
}
pub struct Probe {
    pub resource: ResourceId,
    pub secret: [u8; 32],
    pub key: Option<[u8; 32]>,
}
pub struct Active {
    pub credential: CredentialId,
    pub subject: PrincipalId,
    pub resource: ResourceId,
    pub capabilities: CapabilitySet,
    pub issued: u64,
    pub expires: Option<u64>,
}
pub trait Introspection: Send + Sync {
    fn introspect_key(
        &self,
        probe: Probe,
        issuer: &str,
    ) -> impl Future<Output = Result<Option<Active>, darkhorse_domain::tokens::Error>> + Send;
}
#[cfg(test)]
#[path = "../tests/unit/personal_keys.rs"]
mod tests;
