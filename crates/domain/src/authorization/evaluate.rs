use super::{
    Catalog,
    grants::{current_grants, intersect, scoped_grants},
    model::*,
};
use crate::identity::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denial {
    InactivePrincipal,
    RevokedCredential,
    SubjectMismatch,
    StaleCredential,
    AudienceMismatch,
    InvalidCredential,
    NotYetValid,
    ExpiredCredential,
    UnknownApplication,
    InactiveApplication,
    UnknownResource,
    InactiveResource,
    InvalidAssignment,
    UnknownClient,
    InactiveClient,
    ClientRestriction,
    InvalidScope,
    UnknownCapability,
    InsufficientAccess,
    EmptyGrant,
    DelegationExceedsAccess,
}

#[derive(Debug)]
pub struct Evaluation<'a> {
    pub principal: &'a Principal,
    pub credential: &'a CredentialGrant,
    pub target: Target,
    /// Explicit Unix seconds; never read a clock from the policy engine.
    pub now: u64,
}

/// Evaluate authenticated facts from a coherent current policy revision.
/// This computation cannot establish freshness, authenticity, or database isolation.
pub fn effective_capabilities(
    catalog: &Catalog,
    input: &Evaluation<'_>,
) -> Result<CapabilitySet, Denial> {
    validate_credential(input)?;
    let current = current_grants(catalog, input.principal, input.target)?;
    validate_ceiling(catalog, &input.credential.ceiling)?;
    let bounded = intersect(&current, &input.credential.ceiling);
    match &input.credential.delegation {
        Delegation::PersonalKey => Ok(bounded),
        Delegation::OAuth { client, scopes } => Ok(intersect(
            &bounded,
            &scoped_grants(catalog, input.target, *client, scopes)?,
        )),
    }
}

fn validate_credential(input: &Evaluation<'_>) -> Result<(), Denial> {
    let grant = input.credential;
    if grant.revoked {
        return Err(Denial::RevokedCredential);
    }
    if grant.subject != input.principal.id {
        return Err(Denial::SubjectMismatch);
    }
    if grant.principal_epoch != input.principal.credential_epoch {
        return Err(Denial::StaleCredential);
    }
    if grant.target != input.target {
        return Err(Denial::AudienceMismatch);
    }
    validate_lifetime(grant, input.now)
}

fn validate_lifetime(grant: &CredentialGrant, now: u64) -> Result<(), Denial> {
    match grant.expires_at {
        Some(expiry) if expiry <= grant.valid_from => return Err(Denial::InvalidCredential),
        None if matches!(grant.delegation, Delegation::OAuth { .. }) => {
            return Err(Denial::InvalidCredential);
        }
        _ => {}
    }
    if now < grant.valid_from {
        return Err(Denial::NotYetValid);
    }
    if grant.expires_at.is_some_and(|expiry| now >= expiry) {
        return Err(Denial::ExpiredCredential);
    }
    Ok(())
}

fn validate_ceiling(catalog: &Catalog, ceiling: &CapabilitySet) -> Result<(), Denial> {
    for id in ceiling {
        if !catalog.capabilities.contains_key(id) {
            return Err(Denial::UnknownCapability);
        }
    }
    Ok(())
}

pub fn authorize(
    catalog: &Catalog,
    input: &Evaluation<'_>,
    capability: CapabilityId,
) -> Result<(), Denial> {
    if effective_capabilities(catalog, input)?.contains(&capability) {
        Ok(())
    } else {
        Err(Denial::InsufficientAccess)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/authorization/evaluate.rs"]
mod tests;
