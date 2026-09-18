//! Bounded target projection under the caller's primary security-state fence.
use darkhorse_domain::{AccountStatus, authorization::*, identity::*, tokens::Error};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use std::collections::BTreeSet;
use uuid::Uuid;
mod consent;
pub(super) use consent::{approve, approved};
type Tx<'a> = Transaction<'a, Postgres>;
const MAX_CAPABILITIES: usize = 256;
const MAX_ROLES: usize = 64;
struct Projection {
    target: PgRow,
    capabilities: Vec<Uuid>,
    roles: Vec<PgRow>,
    scopes: Vec<PgRow>,
}

pub(super) async fn plan(
    tx: &mut Tx<'_>,
    principal: PrincipalId,
    client: ClientId,
    audience: &str,
    scopes: &[String],
    limit: Option<&CapabilitySet>,
) -> Result<IssuancePlan, Error> {
    darkhorse_domain::tokens::profile(scopes, Some(audience))?;
    let row = sqlx::query("SELECT r.id,r.application_id,a.active,c.active AS client_active,p.active AS principal_active,p.credential_epoch FROM protected_resources r JOIN applications a ON a.id=r.application_id JOIN client_resources cr ON cr.resource_id=r.id AND cr.client_id=$1 JOIN oauth_clients c ON c.id=cr.client_id JOIN principals p ON p.id=$2 WHERE r.audience=$3")
        .bind(uuid(client.as_u128())).bind(uuid(principal.as_u128())).bind(audience)
        .fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(Error::InvalidGrant)?;
    let resource: Uuid = row.try_get("id").map_err(storage)?;
    let application: Uuid = row.try_get("application_id").map_err(storage)?;
    let capabilities: Vec<Uuid> = sqlx::query_scalar("SELECT rc.capability_id FROM resource_capabilities rc JOIN capabilities c ON c.id=rc.capability_id AND NOT c.retired WHERE rc.application_id=$1 AND rc.resource_id=$2 ORDER BY rc.capability_id LIMIT 257")
        .bind(application).bind(resource).fetch_all(&mut **tx).await.map_err(storage)?;
    let roles = sqlx::query("SELECT pr.role_id,ARRAY(SELECT capability_id FROM role_capabilities WHERE role_id=pr.role_id AND capability_id=ANY($3) ORDER BY capability_id) AS capabilities FROM principal_roles pr WHERE pr.principal_id=$1 AND pr.application_id=$2 ORDER BY pr.role_id LIMIT 65")
        .bind(uuid(principal.as_u128())).bind(application).bind(&capabilities).fetch_all(&mut **tx).await.map_err(storage)?;
    let selected = sqlx::query("SELECT s.id,ARRAY(SELECT capability_id FROM scope_capabilities WHERE scope_id=s.id AND capability_id=ANY($4) ORDER BY capability_id) AS capabilities FROM resource_scopes s JOIN client_scopes cs ON cs.scope_id=s.id AND cs.client_id=$1 WHERE s.resource_id=$2 AND s.name=ANY($3) ORDER BY s.id LIMIT 33")
        .bind(uuid(client.as_u128())).bind(resource).bind(scopes).bind(&capabilities).fetch_all(&mut **tx).await.map_err(storage)?;
    assemble(
        &Projection {
            target: row,
            capabilities,
            roles,
            scopes: selected,
        },
        principal,
        client,
        scopes,
        limit,
    )
}
fn assemble(
    projection: &Projection,
    principal: PrincipalId,
    client: ClientId,
    names: &[String],
    limit: Option<&CapabilitySet>,
) -> Result<IssuancePlan, Error> {
    let Projection {
        target: row,
        capabilities: caps,
        roles,
        scopes,
    } = projection;
    if caps.len() > MAX_CAPABILITIES || roles.len() > MAX_ROLES {
        return Err(Error::Unavailable);
    }
    if scopes.len() + 1 != names.len() {
        return Err(Error::InvalidGrant);
    }
    let application = ApplicationId::from_u128(id(row, "application_id")?).map_err(storage)?;
    let resource = ResourceId::from_u128(id(row, "id")?).map_err(storage)?;
    let exposed = capabilities(caps)?;
    let role_defs = roles
        .iter()
        .map(|r| role(r, application))
        .collect::<Result<Vec<_>, _>>()?;
    let scope_defs = scopes
        .iter()
        .map(|s| scope(s, resource))
        .collect::<Result<Vec<_>, _>>()?;
    let selected = scope_defs.iter().map(|s| s.id).collect();
    let subject = Principal {
        id: principal,
        status: if row
            .try_get::<bool, _>("principal_active")
            .map_err(storage)?
        {
            AccountStatus::Active
        } else {
            AccountStatus::Inactive
        },
        credential_epoch: row
            .try_get::<i64, _>("credential_epoch")
            .map_err(storage)?
            .try_into()
            .map_err(storage)?,
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
            active: row.try_get("active").map_err(storage)?,
        }],
        resources: vec![Resource {
            id: resource,
            application,
            active: true,
            capabilities: exposed.clone(),
        }],
        capabilities: exposed
            .iter()
            .map(|&id| Capability {
                id,
                applications: BTreeSet::from([application]),
            })
            .collect(),
        roles: role_defs,
        scopes: scope_defs,
        clients: vec![Client {
            id: client,
            application,
            active: row.try_get("client_active").map_err(storage)?,
            resources: BTreeSet::from([resource]),
            scopes: selected,
        }],
    })
    .map_err(storage)?;
    let selected = scopes
        .iter()
        .map(|s| ScopeId::from_u128(id(s, "id")?).map_err(storage))
        .collect::<Result<_, _>>()?;
    plan_oauth(
        &catalog,
        &subject,
        Target {
            application,
            resource,
        },
        client,
        selected,
        limit.unwrap_or(&exposed),
    )
    .map_err(|_| Error::InvalidGrant)
}
fn role(row: &PgRow, application: ApplicationId) -> Result<Role, Error> {
    Ok(Role {
        id: RoleId::from_u128(id(row, "role_id")?).map_err(storage)?,
        applications: BTreeSet::from([application]),
        capabilities: capabilities(
            &row.try_get::<Vec<Uuid>, _>("capabilities")
                .map_err(storage)?,
        )?,
    })
}
fn scope(row: &PgRow, resource: ResourceId) -> Result<Scope, Error> {
    Ok(Scope {
        id: ScopeId::from_u128(id(row, "id")?).map_err(storage)?,
        resource,
        capabilities: capabilities(
            &row.try_get::<Vec<Uuid>, _>("capabilities")
                .map_err(storage)?,
        )?,
    })
}
pub(in crate::postgres) fn capabilities(values: &[Uuid]) -> Result<CapabilitySet, Error> {
    values
        .iter()
        .map(|v| CapabilityId::from_u128(v.as_u128()).map_err(storage))
        .collect()
}
pub(in crate::postgres) fn encoded(values: &CapabilitySet) -> Vec<Uuid> {
    values.iter().map(|v| uuid(v.as_u128())).collect()
}
fn id(row: &PgRow, field: &str) -> Result<u128, Error> {
    Ok(row.try_get::<Uuid, _>(field).map_err(storage)?.as_u128())
}
fn uuid(id: u128) -> Uuid {
    Uuid::from_u128(id)
}
fn storage<T>(_: T) -> Error {
    Error::Unavailable
}
