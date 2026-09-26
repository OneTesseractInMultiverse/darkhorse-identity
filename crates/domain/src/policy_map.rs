//! Read-only, application-scoped structural projection of authorization policy.
//!
//! The projection contains only catalog bindings and grants present in the input
//! snapshot. It does not calculate effective access for users or clients.

use crate::{
    admin_catalog::PermissionDefinition,
    identity::{ApplicationId, CapabilityId, ResourceId, RoleId, ScopeId},
    registration::{Label, ScopeName},
};
use std::collections::BTreeSet;

/// Projection work bound, including the application; this is not a capacity claim.
pub const MAX_NODES: usize = 2_048;
/// Projection work bound for explicit and structural edges; this is not a capacity claim.
pub const MAX_EDGES: usize = 8_192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidSnapshot,
    TooLarge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Application {
    pub id: ApplicationId,
    pub name: Label,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub id: RoleId,
    pub name: Label,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    pub id: CapabilityId,
    pub definition: PermissionDefinition,
    pub retired: bool,
}

/// Source row. The audience is a stable protocol-facing resource key shown in the inspector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    pub id: ResourceId,
    pub application: ApplicationId,
    pub name: Label,
    pub audience: String,
}

/// Source row. The application and resource references are checked and then type-qualified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub id: ScopeId,
    pub application: ApplicationId,
    pub resource: ResourceId,
    pub name: ScopeName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A coherent, application-scoped input. `roles` and `capabilities` must already
/// be filtered to definitions explicitly bound to `application`.
pub struct Snapshot {
    pub application: Application,
    pub policy_revision: u64,
    pub roles: Vec<Role>,
    pub capabilities: Vec<Capability>,
    pub resources: Vec<Resource>,
    pub scopes: Vec<Scope>,
    pub role_capabilities: Vec<(RoleId, CapabilityId)>,
    pub resource_capabilities: Vec<(ResourceId, CapabilityId)>,
    pub scope_capabilities: Vec<(ScopeId, CapabilityId)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NodeId {
    Application(ApplicationId),
    Role(RoleId),
    Capability(CapabilityId),
    Resource(ResourceId),
    Scope(ScopeId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Relationship {
    ApplicationRole,
    ApplicationCapability,
    ApplicationResource,
    RoleCapability,
    ResourceCapability,
    ResourceScope,
    ScopeCapability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    pub from: NodeId,
    pub relationship: Relationship,
    pub to: NodeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceNode {
    pub id: ResourceId,
    pub name: Label,
    pub audience: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeNode {
    pub id: ScopeId,
    pub resource: ResourceId,
    pub name: ScopeName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Graph {
    pub application: Application,
    pub policy_revision: u64,
    pub roles: Vec<Role>,
    pub capabilities: Vec<Capability>,
    pub resources: Vec<ResourceNode>,
    pub scopes: Vec<ScopeNode>,
    pub edges: Vec<Edge>,
}

/// Validate and deterministically project one bounded application policy snapshot.
pub fn project(snapshot: Snapshot) -> Result<Graph, Error> {
    let node_count = 1usize
        .saturating_add(snapshot.roles.len())
        .saturating_add(snapshot.capabilities.len())
        .saturating_add(snapshot.resources.len())
        .saturating_add(snapshot.scopes.len());
    let edge_count = snapshot
        .roles
        .len()
        .saturating_add(snapshot.capabilities.len())
        .saturating_add(snapshot.resources.len())
        .saturating_add(snapshot.scopes.len())
        .saturating_add(snapshot.role_capabilities.len())
        .saturating_add(snapshot.resource_capabilities.len())
        .saturating_add(snapshot.scope_capabilities.len());
    if node_count > MAX_NODES || edge_count > MAX_EDGES {
        return Err(Error::TooLarge);
    }
    if snapshot.policy_revision > i64::MAX as u64 {
        return Err(Error::InvalidSnapshot);
    }

    let role_ids = unique_ids(snapshot.roles.iter().map(|role| role.id))?;
    let capability_ids = unique_ids(snapshot.capabilities.iter().map(|capability| capability.id))?;
    let resource_ids = unique_ids(snapshot.resources.iter().map(|resource| resource.id))?;
    let scope_ids = unique_ids(snapshot.scopes.iter().map(|scope| scope.id))?;
    let application_id = snapshot.application.id;

    if snapshot
        .resources
        .iter()
        .any(|resource| resource.application != application_id)
        || snapshot.scopes.iter().any(|scope| {
            scope.application != application_id || !resource_ids.contains(&scope.resource)
        })
        || !unique_pairs(&snapshot.role_capabilities)
        || !unique_pairs(&snapshot.resource_capabilities)
        || !unique_pairs(&snapshot.scope_capabilities)
    {
        return Err(Error::InvalidSnapshot);
    }

    let retired_capabilities = snapshot
        .capabilities
        .iter()
        .filter(|capability| capability.retired)
        .map(|capability| capability.id)
        .collect::<BTreeSet<_>>();
    if snapshot.role_capabilities.iter().any(|(role, capability)| {
        !role_ids.contains(role)
            || !capability_ids.contains(capability)
            || retired_capabilities.contains(capability)
    }) || snapshot
        .resource_capabilities
        .iter()
        .any(|(resource, capability)| {
            !resource_ids.contains(resource)
                || !capability_ids.contains(capability)
                || retired_capabilities.contains(capability)
        })
        || snapshot
            .scope_capabilities
            .iter()
            .any(|(scope, capability)| {
                !scope_ids.contains(scope)
                    || !capability_ids.contains(capability)
                    || retired_capabilities.contains(capability)
            })
    {
        return Err(Error::InvalidSnapshot);
    }

    let app_node = NodeId::Application(application_id);
    let mut edges = Vec::with_capacity(edge_count);
    edges.extend(snapshot.roles.iter().map(|role| Edge {
        from: app_node,
        relationship: Relationship::ApplicationRole,
        to: NodeId::Role(role.id),
    }));
    edges.extend(snapshot.capabilities.iter().map(|capability| Edge {
        from: app_node,
        relationship: Relationship::ApplicationCapability,
        to: NodeId::Capability(capability.id),
    }));
    edges.extend(snapshot.resources.iter().map(|resource| Edge {
        from: app_node,
        relationship: Relationship::ApplicationResource,
        to: NodeId::Resource(resource.id),
    }));
    edges.extend(
        snapshot
            .role_capabilities
            .iter()
            .map(|(role, capability)| Edge {
                from: NodeId::Role(*role),
                relationship: Relationship::RoleCapability,
                to: NodeId::Capability(*capability),
            }),
    );
    edges.extend(
        snapshot
            .resource_capabilities
            .iter()
            .map(|(resource, capability)| Edge {
                from: NodeId::Resource(*resource),
                relationship: Relationship::ResourceCapability,
                to: NodeId::Capability(*capability),
            }),
    );
    edges.extend(snapshot.scopes.iter().map(|scope| Edge {
        from: NodeId::Resource(scope.resource),
        relationship: Relationship::ResourceScope,
        to: NodeId::Scope(scope.id),
    }));
    edges.extend(
        snapshot
            .scope_capabilities
            .iter()
            .map(|(scope, capability)| Edge {
                from: NodeId::Scope(*scope),
                relationship: Relationship::ScopeCapability,
                to: NodeId::Capability(*capability),
            }),
    );
    edges.sort_by_key(|edge| (edge.relationship, edge.from, edge.to));

    let mut roles = snapshot.roles;
    roles.sort_by_key(|role| role.id);
    let mut capabilities = snapshot.capabilities;
    capabilities.sort_by_key(|capability| capability.id);
    let mut resources = snapshot
        .resources
        .into_iter()
        .map(|resource| ResourceNode {
            id: resource.id,
            name: resource.name,
            audience: resource.audience,
        })
        .collect::<Vec<_>>();
    resources.sort_by_key(|resource| resource.id);
    let mut scopes = snapshot
        .scopes
        .into_iter()
        .map(|scope| ScopeNode {
            id: scope.id,
            resource: scope.resource,
            name: scope.name,
        })
        .collect::<Vec<_>>();
    scopes.sort_by_key(|scope| scope.id);

    Ok(Graph {
        application: snapshot.application,
        policy_revision: snapshot.policy_revision,
        roles,
        capabilities,
        resources,
        scopes,
        edges,
    })
}

fn unique_ids<T: Ord>(ids: impl Iterator<Item = T>) -> Result<BTreeSet<T>, Error> {
    let mut unique = BTreeSet::new();
    for id in ids {
        if !unique.insert(id) {
            return Err(Error::InvalidSnapshot);
        }
    }
    Ok(unique)
}

fn unique_pairs<A: Ord + Copy, B: Ord + Copy>(pairs: &[(A, B)]) -> bool {
    pairs.iter().copied().collect::<BTreeSet<_>>().len() == pairs.len()
}

#[cfg(test)]
#[path = "../tests/unit/policy_map.rs"]
mod tests;
