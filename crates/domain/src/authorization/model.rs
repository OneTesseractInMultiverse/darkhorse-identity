use crate::{AccountStatus, identity::*};
use std::collections::BTreeSet;

pub type CapabilitySet = BTreeSet<CapabilityId>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Target {
    pub application: ApplicationId,
    pub resource: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Application {
    pub id: ApplicationId,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    pub id: ResourceId,
    pub application: ApplicationId,
    pub active: bool,
    pub capabilities: CapabilitySet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    pub id: CapabilityId,
    /// Explicit bindings; an empty set exposes the definition nowhere.
    pub applications: BTreeSet<ApplicationId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub id: RoleId,
    pub applications: BTreeSet<ApplicationId>,
    pub capabilities: CapabilitySet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub id: ScopeId,
    pub resource: ResourceId,
    pub capabilities: CapabilitySet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    pub id: ClientId,
    pub application: ApplicationId,
    pub active: bool,
    pub resources: BTreeSet<ResourceId>,
    pub scopes: BTreeSet<ScopeId>,
}

#[derive(Debug, Clone, Default)]
pub struct Definitions {
    pub applications: Vec<Application>,
    pub resources: Vec<Resource>,
    pub capabilities: Vec<Capability>,
    pub roles: Vec<Role>,
    pub scopes: Vec<Scope>,
    pub clients: Vec<Client>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Assignment {
    pub application: ApplicationId,
    pub role: RoleId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub id: PrincipalId,
    pub status: AccountStatus,
    /// Advance on revoke-all/deactivation; old credentials must never be revived.
    pub credential_epoch: u64,
    pub assignments: BTreeSet<Assignment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delegation {
    PersonalKey,
    OAuth {
        client: ClientId,
        scopes: BTreeSet<ScopeId>,
    },
}

/// One explicit application/resource grant of an authenticated stored credential.
/// Protocol parsing and secret verification happen before constructing these facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialGrant {
    pub credential: CredentialId,
    pub subject: PrincipalId,
    pub target: Target,
    pub revoked: bool,
    pub principal_epoch: u64,
    pub valid_from: u64,
    pub expires_at: Option<u64>,
    pub ceiling: CapabilitySet,
    pub delegation: Delegation,
}
