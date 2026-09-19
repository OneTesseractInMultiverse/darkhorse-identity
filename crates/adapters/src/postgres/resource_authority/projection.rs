//! Decode a bounded adapter projection into framework-free policy inputs.
use super::*;
const MAX_CAPABILITIES: usize = 256;
const MAX_ROLES: usize = 64;

#[derive(sqlx::FromRow)]
pub(super) struct Projection {
    resource_id: Uuid,
    application_id: Uuid,
    active: bool,
    client_active: bool,
    principal_active: bool,
    credential_epoch: i64,
    capabilities: Vec<Uuid>,
    roles: Vec<(Uuid, Vec<Uuid>)>,
    scopes: Vec<(Uuid, Vec<Uuid>)>,
    historical: Vec<Uuid>,
}
pub(super) fn assemble(
    projection: &Projection,
    principal: PrincipalId,
    client: ClientId,
    names: &[String],
) -> Result<Policy, Error> {
    let Projection {
        capabilities: caps,
        roles,
        scopes,
        historical,
        ..
    } = projection;
    if caps.len() > MAX_CAPABILITIES
        || roles.len() > MAX_ROLES
        || historical.len() > MAX_CAPABILITIES
    {
        return Err(Error::Unavailable);
    }
    if scopes.len() + 1 != names.len() {
        return Err(Error::InvalidGrant);
    }
    let application =
        ApplicationId::from_u128(projection.application_id.as_u128()).map_err(storage)?;
    let resource = ResourceId::from_u128(projection.resource_id.as_u128()).map_err(storage)?;
    let exposed = capabilities(caps)?;
    let known: CapabilitySet = exposed.union(&capabilities(historical)?).copied().collect();
    let role_defs = roles
        .iter()
        .map(|r| role(r, application))
        .collect::<Result<Vec<_>, _>>()?;
    let scope_defs = scopes
        .iter()
        .map(|s| scope(s, resource))
        .collect::<Result<Vec<_>, _>>()?;
    let selected: BTreeSet<_> = scope_defs.iter().map(|s| s.id).collect();
    let subject = Principal {
        id: principal,
        status: if projection.principal_active {
            AccountStatus::Active
        } else {
            AccountStatus::Inactive
        },
        credential_epoch: projection.credential_epoch.try_into().map_err(storage)?,
        assignments: role_defs
            .iter()
            .map(|r| Assignment {
                application,
                role: r.id,
            })
            .collect(),
    };
    let catalog = Catalog::new(Definitions {
        applications: vec![Application {
            id: application,
            active: projection.active,
        }],
        resources: vec![Resource {
            id: resource,
            application,
            active: true,
            capabilities: exposed.clone(),
        }],
        capabilities: known
            .iter()
            .map(|&id| Capability {
                id,
                applications: if exposed.contains(&id) {
                    BTreeSet::from([application])
                } else {
                    BTreeSet::new()
                },
            })
            .collect(),
        roles: role_defs,
        scopes: scope_defs,
        clients: vec![Client {
            id: client,
            application,
            active: projection.client_active,
            resources: BTreeSet::from([resource]),
            scopes: selected.clone(),
        }],
    })
    .map_err(storage)?;
    Ok(Policy {
        catalog,
        principal: subject,
        target: Target {
            application,
            resource,
        },
        client,
        scopes: selected,
        exposed,
    })
}
fn role((id, caps): &(Uuid, Vec<Uuid>), application: ApplicationId) -> Result<Role, Error> {
    Ok(Role {
        id: RoleId::from_u128(id.as_u128()).map_err(storage)?,
        applications: BTreeSet::from([application]),
        capabilities: capabilities(caps)?,
    })
}
fn scope((id, caps): &(Uuid, Vec<Uuid>), resource: ResourceId) -> Result<Scope, Error> {
    Ok(Scope {
        id: ScopeId::from_u128(id.as_u128()).map_err(storage)?,
        resource,
        capabilities: capabilities(caps)?,
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/postgres/resource_authority/projection.rs"]
mod tests;
