use super::*;
use darkhorse_application::admin_catalog::PolicyMapError;
use darkhorse_domain::{
    admin_catalog::PermissionDefinition,
    policy_map::{self, Application, Capability, Resource, Role, Scope, Snapshot},
    registration::{Label, ScopeName},
};

pub(super) async fn read(
    tx: &mut Tx<'_>,
    application: ApplicationId,
) -> Result<policy_map::Graph, PolicyMapError> {
    let row = sqlx::query("SELECT id,name,active FROM applications WHERE id=$1")
        .bind(uuid(application.as_u128()))
        .fetch_optional(&mut **tx)
        .await
        .map_err(unavailable)?
        .ok_or(PolicyMapError::NotFound)?;
    let application = Application {
        id: ApplicationId::from_u128(identifier(&row, "id").map_err(unavailable)?)
            .map_err(|_| PolicyMapError::Unavailable)?,
        name: Label::new(&row.try_get::<String, _>("name").map_err(unavailable)?)
            .map_err(|_| PolicyMapError::Unavailable)?,
        active: row.try_get("active").map_err(unavailable)?,
    };

    let mut remaining_nodes = policy_map::MAX_NODES - 1;
    let role_rows = bounded_query(
        tx,
        "SELECT r.id,r.name FROM roles r JOIN role_applications b ON b.role_id=r.id WHERE b.application_id=$1 ORDER BY r.id LIMIT $2",
        application.id,
        &mut remaining_nodes,
    )
    .await?;
    let capability_rows = bounded_query(
        tx,
        "SELECT c.id,c.permission_key,c.meaning,c.retired FROM capabilities c JOIN capability_applications b ON b.capability_id=c.id WHERE b.application_id=$1 ORDER BY c.id LIMIT $2",
        application.id,
        &mut remaining_nodes,
    )
    .await?;
    let resource_rows = bounded_query(
        tx,
        "SELECT id,application_id,name,audience FROM protected_resources WHERE application_id=$1 ORDER BY id LIMIT $2",
        application.id,
        &mut remaining_nodes,
    )
    .await?;
    let scope_rows = bounded_query(
        tx,
        "SELECT id,application_id,resource_id,name FROM resource_scopes WHERE application_id=$1 ORDER BY id LIMIT $2",
        application.id,
        &mut remaining_nodes,
    )
    .await?;

    let roles = role_rows.iter().map(role).collect::<Result<Vec<_>, _>>()?;
    let capabilities = capability_rows
        .iter()
        .map(capability)
        .collect::<Result<Vec<_>, _>>()?;
    let resources = resource_rows
        .iter()
        .map(resource)
        .collect::<Result<Vec<_>, _>>()?;
    let scopes = scope_rows
        .iter()
        .map(scope)
        .collect::<Result<Vec<_>, _>>()?;

    let base_edges = roles.len() + capabilities.len() + resources.len() + scopes.len();
    let mut remaining_edges = policy_map::MAX_EDGES - base_edges;
    let role_capabilities = bounded_query(
        tx,
        "SELECT g.role_id,g.capability_id FROM role_capabilities g JOIN role_applications r ON r.role_id=g.role_id JOIN capability_applications c ON c.capability_id=g.capability_id AND c.application_id=r.application_id WHERE r.application_id=$1 ORDER BY g.role_id,g.capability_id LIMIT $2",
        application.id,
        &mut remaining_edges,
    )
    .await?
    .iter()
    .map(role_capability)
    .collect::<Result<Vec<_>, _>>()?;
    let resource_capabilities = bounded_query(
        tx,
        "SELECT resource_id,capability_id FROM resource_capabilities WHERE application_id=$1 ORDER BY resource_id,capability_id LIMIT $2",
        application.id,
        &mut remaining_edges,
    )
    .await?
    .iter()
    .map(resource_capability)
    .collect::<Result<Vec<_>, _>>()?;
    let scope_capabilities = bounded_query(
        tx,
        "SELECT g.scope_id,g.capability_id FROM scope_capabilities g JOIN resource_scopes s ON s.id=g.scope_id AND s.application_id=g.application_id WHERE s.application_id=$1 ORDER BY g.scope_id,g.capability_id LIMIT $2",
        application.id,
        &mut remaining_edges,
    )
    .await?
    .iter()
    .map(scope_capability)
    .collect::<Result<Vec<_>, _>>()?;

    let policy_revision = reads::revision(tx).await.map_err(unavailable)?;
    policy_map::project(Snapshot {
        application,
        policy_revision,
        roles,
        capabilities,
        resources,
        scopes,
        role_capabilities,
        resource_capabilities,
        scope_capabilities,
    })
    .map_err(|error| match error {
        policy_map::Error::InvalidSnapshot => PolicyMapError::Unavailable,
        policy_map::Error::TooLarge => PolicyMapError::TooLarge,
    })
}

async fn bounded_query(
    tx: &mut Tx<'_>,
    sql: &'static str,
    application: ApplicationId,
    remaining: &mut usize,
) -> Result<Vec<PgRow>, PolicyMapError> {
    let rows = sqlx::query(sql)
        .bind(uuid(application.as_u128()))
        .bind(*remaining as i64 + 1)
        .fetch_all(&mut **tx)
        .await
        .map_err(unavailable)?;
    if rows.len() > *remaining {
        return Err(PolicyMapError::TooLarge);
    }
    *remaining -= rows.len();
    Ok(rows)
}

fn role(row: &PgRow) -> Result<Role, PolicyMapError> {
    Ok(Role {
        id: RoleId::from_u128(identifier(row, "id").map_err(unavailable)?)
            .map_err(|_| PolicyMapError::Unavailable)?,
        name: Label::new(&row.try_get::<String, _>("name").map_err(unavailable)?)
            .map_err(|_| PolicyMapError::Unavailable)?,
    })
}

fn capability(row: &PgRow) -> Result<Capability, PolicyMapError> {
    Ok(Capability {
        id: CapabilityId::from_u128(identifier(row, "id").map_err(unavailable)?)
            .map_err(|_| PolicyMapError::Unavailable)?,
        definition: PermissionDefinition::new(
            &row.try_get::<String, _>("permission_key")
                .map_err(unavailable)?,
            &row.try_get::<String, _>("meaning").map_err(unavailable)?,
        )
        .map_err(|_| PolicyMapError::Unavailable)?,
        retired: row.try_get("retired").map_err(unavailable)?,
    })
}

fn resource(row: &PgRow) -> Result<Resource, PolicyMapError> {
    Ok(Resource {
        id: ResourceId::from_u128(identifier(row, "id").map_err(unavailable)?)
            .map_err(|_| PolicyMapError::Unavailable)?,
        application: ApplicationId::from_u128(
            identifier(row, "application_id").map_err(unavailable)?,
        )
        .map_err(|_| PolicyMapError::Unavailable)?,
        name: Label::new(&row.try_get::<String, _>("name").map_err(unavailable)?)
            .map_err(|_| PolicyMapError::Unavailable)?,
        audience: row.try_get("audience").map_err(unavailable)?,
    })
}

fn scope(row: &PgRow) -> Result<Scope, PolicyMapError> {
    Ok(Scope {
        id: ScopeId::from_u128(identifier(row, "id").map_err(unavailable)?)
            .map_err(|_| PolicyMapError::Unavailable)?,
        application: ApplicationId::from_u128(
            identifier(row, "application_id").map_err(unavailable)?,
        )
        .map_err(|_| PolicyMapError::Unavailable)?,
        resource: ResourceId::from_u128(identifier(row, "resource_id").map_err(unavailable)?)
            .map_err(|_| PolicyMapError::Unavailable)?,
        name: ScopeName::new(&row.try_get::<String, _>("name").map_err(unavailable)?)
            .map_err(|_| PolicyMapError::Unavailable)?,
    })
}

fn role_capability(row: &PgRow) -> Result<(RoleId, CapabilityId), PolicyMapError> {
    Ok((
        RoleId::from_u128(identifier(row, "role_id").map_err(unavailable)?)
            .map_err(|_| PolicyMapError::Unavailable)?,
        CapabilityId::from_u128(identifier(row, "capability_id").map_err(unavailable)?)
            .map_err(|_| PolicyMapError::Unavailable)?,
    ))
}

fn resource_capability(row: &PgRow) -> Result<(ResourceId, CapabilityId), PolicyMapError> {
    Ok((
        ResourceId::from_u128(identifier(row, "resource_id").map_err(unavailable)?)
            .map_err(|_| PolicyMapError::Unavailable)?,
        CapabilityId::from_u128(identifier(row, "capability_id").map_err(unavailable)?)
            .map_err(|_| PolicyMapError::Unavailable)?,
    ))
}

fn scope_capability(row: &PgRow) -> Result<(ScopeId, CapabilityId), PolicyMapError> {
    Ok((
        ScopeId::from_u128(identifier(row, "scope_id").map_err(unavailable)?)
            .map_err(|_| PolicyMapError::Unavailable)?,
        CapabilityId::from_u128(identifier(row, "capability_id").map_err(unavailable)?)
            .map_err(|_| PolicyMapError::Unavailable)?,
    ))
}

fn unavailable<T>(_: T) -> PolicyMapError {
    PolicyMapError::Unavailable
}
