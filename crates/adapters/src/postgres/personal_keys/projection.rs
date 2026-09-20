use super::*;
#[derive(sqlx::FromRow)]
struct Projection {
    resource_id: Uuid,
    application_id: Uuid,
    resource_name: String,
    application_name: String,
    active: bool,
    principal_active: bool,
    credential_epoch: i64,
    capabilities: Vec<Uuid>,
    roles: Vec<(Uuid, Vec<Uuid>)>,
    historical: Vec<Uuid>,
}
pub(super) struct Policy {
    pub catalog: Catalog,
    pub principal: Principal,
    pub target: Target,
    exposed: CapabilitySet,
    pub application_name: String,
    pub resource_name: String,
}
impl Policy {
    pub fn plan(
        &self,
        application: ApplicationId,
        selection: CapabilitySelection,
    ) -> Result<IssuancePlan, Error> {
        if application != self.target.application {
            return Err(Error::Forbidden);
        }
        plan_key(
            &self.catalog,
            &self.principal,
            self.target,
            selection,
            &self.exposed,
        )
        .map_err(|_| Error::Forbidden)
    }
}
pub(super) async fn load(
    tx: &mut Tx<'_>,
    principal: PrincipalId,
    resource: ResourceId,
    historical: &CapabilitySet,
) -> Result<Policy, Error> {
    let row = sqlx::query_as::<_, Projection>(include_str!("projection.sql"))
        .bind(uuid(principal.as_u128()))
        .bind(uuid(resource.as_u128()))
        .bind(encoded(historical))
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?
        .ok_or(Error::Forbidden)?;
    assemble(row, principal)
}
fn assemble(row: Projection, principal: PrincipalId) -> Result<Policy, Error> {
    if row.capabilities.len() > 256 || row.roles.len() > 64 || row.historical.len() > 256 {
        return Err(Error::Unavailable);
    }
    let application = ApplicationId::from_u128(row.application_id.as_u128()).map_err(storage)?;
    let resource = ResourceId::from_u128(row.resource_id.as_u128()).map_err(storage)?;
    let exposed = capabilities(&row.capabilities)?;
    let known: CapabilitySet = exposed
        .union(&capabilities(&row.historical)?)
        .copied()
        .collect();
    let roles = row
        .roles
        .iter()
        .map(|(id, caps)| {
            Ok(Role {
                id: RoleId::from_u128(id.as_u128()).map_err(storage)?,
                applications: [application].into(),
                capabilities: capabilities(caps)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let subject = Principal {
        id: principal,
        status: if row.principal_active {
            AccountStatus::Active
        } else {
            AccountStatus::Inactive
        },
        credential_epoch: row.credential_epoch.try_into().map_err(storage)?,
        assignments: roles
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
            active: row.active,
        }],
        resources: vec![Resource {
            id: resource,
            application,
            active: true,
            capabilities: exposed.clone(),
        }],
        capabilities: known
            .iter()
            .map(|&id| darkhorse_domain::authorization::Capability {
                id,
                applications: if exposed.contains(&id) {
                    [application].into()
                } else {
                    BTreeSet::new()
                },
            })
            .collect(),
        roles,
        ..Definitions::default()
    })
    .map_err(storage)?;
    Ok(Policy {
        catalog,
        principal: subject,
        target: Target {
            application,
            resource,
        },
        exposed,
        application_name: row.application_name,
        resource_name: row.resource_name,
    })
}
pub(super) fn capabilities(ids: &[Uuid]) -> Result<CapabilitySet, Error> {
    ids.iter()
        .map(|id| CapabilityId::from_u128(id.as_u128()).map_err(storage))
        .collect()
}
pub(super) fn encoded(caps: &CapabilitySet) -> Vec<Uuid> {
    caps.iter().map(|id| uuid(id.as_u128())).collect()
}
#[cfg(test)]
#[path = "../../../tests/unit/postgres/personal_keys/projection.rs"]
mod tests;
