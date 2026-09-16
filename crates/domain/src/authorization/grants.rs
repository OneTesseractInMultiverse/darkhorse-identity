//! Current authority computations shared by evaluation and issuance planning.
use super::{Catalog, Denial, model::*};
use crate::{AccountStatus, identity::*};
use std::collections::BTreeSet;

pub(super) fn current_grants(
    catalog: &Catalog,
    principal: &Principal,
    target: Target,
) -> Result<CapabilitySet, Denial> {
    if principal.status != AccountStatus::Active {
        return Err(Denial::InactivePrincipal);
    }
    let resource = active_target(catalog, target)?;
    let mut grants = CapabilitySet::new();
    for assignment in &principal.assignments {
        let role = assigned_role(catalog, assignment)?;
        if assignment.application == target.application {
            grants.extend(&role.capabilities);
        }
    }
    Ok(intersect(&grants, &resource.capabilities))
}

fn assigned_role<'a>(catalog: &'a Catalog, assignment: &Assignment) -> Result<&'a Role, Denial> {
    if !catalog.applications.contains_key(&assignment.application) {
        return Err(Denial::InvalidAssignment);
    }
    let role = catalog
        .roles
        .get(&assignment.role)
        .ok_or(Denial::InvalidAssignment)?;
    if !role.applications.contains(&assignment.application) {
        return Err(Denial::InvalidAssignment);
    }
    Ok(role)
}

fn active_target(catalog: &Catalog, target: Target) -> Result<&Resource, Denial> {
    active_application(catalog, target.application)?;
    let resource = catalog
        .resources
        .get(&target.resource)
        .ok_or(Denial::UnknownResource)?;
    if resource.application != target.application {
        return Err(Denial::AudienceMismatch);
    }
    if !resource.active {
        return Err(Denial::InactiveResource);
    }
    Ok(resource)
}

fn active_application(catalog: &Catalog, id: ApplicationId) -> Result<(), Denial> {
    let application = catalog
        .applications
        .get(&id)
        .ok_or(Denial::UnknownApplication)?;
    if !application.active {
        return Err(Denial::InactiveApplication);
    }
    Ok(())
}

pub(super) fn scoped_grants(
    catalog: &Catalog,
    target: Target,
    client_id: ClientId,
    scopes: &BTreeSet<ScopeId>,
) -> Result<CapabilitySet, Denial> {
    let client = catalog
        .clients
        .get(&client_id)
        .ok_or(Denial::UnknownClient)?;
    if !client.active {
        return Err(Denial::InactiveClient);
    }
    active_application(catalog, client.application)?;
    if !client.resources.contains(&target.resource) {
        return Err(Denial::ClientRestriction);
    }
    if scopes.is_empty() {
        return Err(Denial::InvalidScope);
    }
    let mut grants = CapabilitySet::new();
    for &id in scopes {
        grants.extend(&allowed_scope(catalog, client, target.resource, id)?.capabilities);
    }
    Ok(grants)
}

fn allowed_scope<'a>(
    catalog: &'a Catalog,
    client: &Client,
    resource: ResourceId,
    id: ScopeId,
) -> Result<&'a Scope, Denial> {
    if !client.scopes.contains(&id) {
        return Err(Denial::InvalidScope);
    }
    // Catalog validation guarantees that registered scope IDs exist.
    let scope = &catalog.scopes[&id];
    if scope.resource != resource {
        return Err(Denial::InvalidScope);
    }
    Ok(scope)
}

pub(super) fn intersect(left: &CapabilitySet, right: &CapabilitySet) -> CapabilitySet {
    left.intersection(right).copied().collect()
}
