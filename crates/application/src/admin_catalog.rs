//! Typed administrative catalogs; transports cannot supply their own authority facts.
use crate::registration::{ApplicationRecord, Entropy, ResourceRecord, ScopeRecord};
use darkhorse_domain::{
    admin_catalog::{Change, Query},
    identity::*,
    registration::RegistrationError as Error,
};
use std::{future::Future, num::NonZeroU128};
#[derive(Debug, Clone, Copy)]
pub enum List {
    Applications,
    Clients(ApplicationId),
    Resources(ApplicationId),
    Scopes(ApplicationId),
    Capabilities(Option<ApplicationId>),
    Roles(Option<ApplicationId>),
}
#[derive(Debug, Clone, Copy)]
pub enum Target {
    Capability(CapabilityId),
    Role(RoleId),
    Resource(ApplicationId, ResourceId),
    Scope(ApplicationId, ResourceId, ScopeId),
}
#[derive(Debug, Clone)]
pub struct ClientSummary {
    pub id: ClientId,
    pub application: ApplicationId,
    pub name: String,
    pub active: bool,
    pub revision: u64,
}
#[derive(Debug, Clone)]
pub struct CapabilitySummary {
    pub id: CapabilityId,
    pub key: String,
    pub meaning: String,
    pub retired: bool,
}
#[derive(Debug, Clone)]
pub struct RoleSummary {
    pub id: RoleId,
    pub name: String,
}
#[derive(Debug, Clone)]
pub enum Item {
    Application(ApplicationRecord),
    Client(ClientSummary),
    Resource(ResourceRecord),
    Scope(ScopeRecord),
    Capability(CapabilitySummary),
    Role(RoleSummary),
}
impl Item {
    pub fn id(&self) -> NonZeroU128 {
        let id = match self {
            Self::Application(r) => r.id.as_u128(),
            Self::Client(r) => r.id.as_u128(),
            Self::Resource(r) => r.id.as_u128(),
            Self::Scope(r) => r.id.as_u128(),
            Self::Capability(r) => r.id.as_u128(),
            Self::Role(r) => r.id.as_u128(),
        };
        NonZeroU128::new(id).expect("typed references are nonzero")
    }
}
pub struct Page {
    pub items: Vec<Item>,
    pub next: Option<NonZeroU128>,
    pub policy_revision: u64,
}
pub struct View {
    pub item: Item,
    pub applications: Vec<ApplicationRecord>,
    pub capabilities: Vec<CapabilitySummary>,
    pub policy_revision: u64,
}
pub struct Written {
    pub target: Target,
    pub policy_revision: u64,
}
pub trait CatalogStore: Send + Sync {
    fn list(
        &self,
        actor: [u8; 32],
        target: List,
        query: Query,
    ) -> impl Future<Output = Result<Page, Error>> + Send;
    fn view(
        &self,
        actor: [u8; 32],
        target: Target,
    ) -> impl Future<Output = Result<View, Error>> + Send;
    fn preflight(
        &self,
        actor: [u8; 32],
        revision: u64,
        change: &Change,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn execute(
        &self,
        actor: [u8; 32],
        revision: u64,
        change: Change,
        identifier: Option<NonZeroU128>,
    ) -> impl Future<Output = Result<Written, Error>> + Send;
}
pub trait Catalog: Send + Sync {
    fn list(
        &self,
        actor: [u8; 32],
        target: List,
        query: Query,
    ) -> impl Future<Output = Result<Page, Error>> + Send;
    fn view(
        &self,
        actor: [u8; 32],
        target: Target,
    ) -> impl Future<Output = Result<View, Error>> + Send;
    fn write(
        &self,
        actor: [u8; 32],
        revision: u64,
        change: Change,
    ) -> impl Future<Output = Result<Written, Error>> + Send;
}
pub struct Service<S, E> {
    pub store: S,
    pub entropy: E,
}
impl<S: CatalogStore, E: Entropy> Catalog for Service<S, E> {
    async fn list(&self, actor: [u8; 32], target: List, query: Query) -> Result<Page, Error> {
        self.store.list(actor, target, query).await
    }
    async fn view(&self, actor: [u8; 32], target: Target) -> Result<View, Error> {
        self.store.view(actor, target).await
    }
    async fn write(
        &self,
        actor: [u8; 32],
        revision: u64,
        change: Change,
    ) -> Result<Written, Error> {
        self.store.preflight(actor, revision, &change).await?;
        let identifier = change
            .needs_identifier()
            .then(|| self.entropy.identifier())
            .transpose()?;
        self.store
            .execute(actor, revision, change, identifier)
            .await
    }
}

#[cfg(test)]
#[path = "../tests/unit/admin_catalog.rs"]
mod tests;
