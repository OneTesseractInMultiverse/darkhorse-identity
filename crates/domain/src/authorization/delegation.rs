use super::{
    Catalog, Denial,
    grants::{current_grants, intersect, scoped_grants},
    model::*,
};
use crate::identity::*;
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub enum CapabilitySelection {
    All,
    Subset(CapabilitySet),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuancePlan {
    pub subject: PrincipalId,
    pub target: Target,
    pub principal_epoch: u64,
    pub ceiling: CapabilitySet,
    pub delegation: Delegation,
}

/// Snapshot delegable authority for one explicit resource. The limit is an upper
/// bound supplied by the caller's delegation policy; it can never grant authority.
pub fn plan_key(
    catalog: &Catalog,
    principal: &Principal,
    target: Target,
    selection: CapabilitySelection,
    delegation_limit: &CapabilitySet,
) -> Result<IssuancePlan, Denial> {
    let eligible = intersect(
        &current_grants(catalog, principal, target)?,
        delegation_limit,
    );
    let ceiling = match selection {
        CapabilitySelection::All => nonempty(eligible)?,
        CapabilitySelection::Subset(requested) => attenuate(&eligible, &requested)?,
    };
    Ok(plan(principal, target, ceiling, Delegation::PersonalKey))
}

/// Compute a capability ceiling after the calling use case has established
/// consent. OIDC profile claims and protocol scopes are separate concerns.
pub fn plan_oauth(
    catalog: &Catalog,
    principal: &Principal,
    target: Target,
    client: ClientId,
    scopes: BTreeSet<ScopeId>,
    consent_limit: &CapabilitySet,
) -> Result<IssuancePlan, Denial> {
    let current = current_grants(catalog, principal, target)?;
    let scoped = intersect(&current, &scoped_grants(catalog, target, client, &scopes)?);
    let ceiling = nonempty(intersect(&scoped, consent_limit))?;
    Ok(plan(
        principal,
        target,
        ceiling,
        Delegation::OAuth { client, scopes },
    ))
}

/// Ceiling computation only; the refresh use case must also check credential
/// validity, client binding, current authority, and refresh-token reuse.
pub fn attenuate(
    existing: &CapabilitySet,
    requested: &CapabilitySet,
) -> Result<CapabilitySet, Denial> {
    if !requested.is_subset(existing) {
        return Err(Denial::DelegationExceedsAccess);
    }
    nonempty(requested.clone())
}

fn nonempty(capabilities: CapabilitySet) -> Result<CapabilitySet, Denial> {
    if capabilities.is_empty() {
        Err(Denial::EmptyGrant)
    } else {
        Ok(capabilities)
    }
}

fn plan(
    principal: &Principal,
    target: Target,
    ceiling: CapabilitySet,
    delegation: Delegation,
) -> IssuancePlan {
    IssuancePlan {
        subject: principal.id,
        target,
        principal_epoch: principal.credential_epoch,
        ceiling,
        delegation,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/authorization/delegation.rs"]
mod tests;
