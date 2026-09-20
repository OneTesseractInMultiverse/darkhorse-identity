use super::*;
use darkhorse_application::registration::{ApplicationRecord, ResourceRecord, ScopeRecord};
const APPLICATION: &str = "SELECT a.id,a.name,a.owner_id,p.email AS owner_email,a.active,a.revision FROM applications a JOIN principals p ON p.id=a.owner_id";
pub(super) async fn revision(tx: &mut Tx<'_>) -> Result<u64, Error> {
    sqlx::query_scalar::<_, i64>("SELECT policy_revision FROM security_state WHERE singleton")
        .fetch_one(&mut **tx)
        .await
        .map_err(storage)?
        .try_into()
        .map_err(storage)
}
pub(super) async fn list(tx: &mut Tx<'_>, target: List, q: &Query) -> Result<Page, Error> {
    let (sql, app) = statement(target, q)?;
    if let Some(app) = app {
        application_exists(tx, app).await?;
    }
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(q.after.map(|v| uuid(v.get())))
        .bind(prefix(&q.search))
        .bind(q.active)
        .bind(app.map(|v| uuid(v.as_u128())))
        .bind(i64::from(q.limit) + 1)
        .fetch_all(&mut **tx)
        .await
        .map_err(storage)?;
    let items = rows
        .iter()
        .map(|row| item(target, row))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(page(items, q.limit as usize, revision(tx).await?))
}
fn statement(target: List, q: &Query) -> Result<(String, Option<ApplicationId>), Error> {
    if q.active.is_some()
        && matches!(
            target,
            List::Resources(_) | List::Scopes(_) | List::Roles(_)
        )
    {
        return Err(Error::Invalid);
    }
    let (select,filter,app)=match target {
 List::Applications=>(APPLICATION.to_owned(),"($4::uuid IS NULL)",None),
 List::Clients(app)=>("SELECT a.id,a.application_id,a.name,a.active,a.revision FROM oauth_clients a".into(),"a.application_id=$4",Some(app)),
 List::Resources(app)=>("SELECT a.id,a.application_id,a.name,a.audience,true AS active FROM protected_resources a".into(),"a.application_id=$4",Some(app)),
 List::Scopes(app)=>("SELECT a.id,a.application_id,a.resource_id,a.name,true AS active FROM resource_scopes a".into(),"a.application_id=$4",Some(app)),
 List::Capabilities(app)=>("SELECT a.id,a.permission_key AS name,a.meaning,NOT a.retired AS active FROM capabilities a".into(),"($4::uuid IS NULL OR EXISTS(SELECT 1 FROM capability_applications b WHERE b.capability_id=a.id AND b.application_id=$4))",app),
 List::Roles(app)=>("SELECT a.id,a.name,true AS active FROM roles a".into(),"($4::uuid IS NULL OR EXISTS(SELECT 1 FROM role_applications b WHERE b.role_id=a.id AND b.application_id=$4))",app),
 };
    // All SQL fragments are fixed above; browser input is bound separately.
    Ok((
        format!(
            "SELECT * FROM ({select} WHERE {filter}) a WHERE ($1::uuid IS NULL OR a.id>$1) AND lower(a.name) LIKE lower($2) ESCAPE '\\' AND ($3::boolean IS NULL OR a.active=$3) ORDER BY a.id LIMIT $5"
        ),
        app,
    ))
}
pub(super) fn prefix(value: &str) -> String {
    format!(
        "{}%",
        value
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )
}
fn page(mut items: Vec<Item>, limit: usize, policy_revision: u64) -> Page {
    let more = items.len() > limit;
    items.truncate(limit);
    let next = if more {
        items.last().map(Item::id)
    } else {
        None
    };
    Page {
        items,
        next,
        policy_revision,
    }
}
fn item(target: List, row: &PgRow) -> Result<Item, Error> {
    Ok(match target {
        List::Applications => Item::Application(application(row)?),
        List::Clients(_) => Item::Client(ClientSummary {
            id: ClientId::from_u128(identifier(row, "id")?).map_err(storage)?,
            application: ApplicationId::from_u128(identifier(row, "application_id")?)
                .map_err(storage)?,
            name: row.try_get("name").map_err(storage)?,
            active: row.try_get("active").map_err(storage)?,
            revision: number(row, "revision")?,
        }),
        List::Resources(_) => Item::Resource(resource(row)?),
        List::Scopes(_) => Item::Scope(scope(row)?),
        List::Capabilities(_) => Item::Capability(capability(row)?),
        List::Roles(_) => Item::Role(role(row)?),
    })
}
fn application(row: &PgRow) -> Result<ApplicationRecord, Error> {
    Ok(ApplicationRecord {
        id: ApplicationId::from_u128(identifier(row, "id")?).map_err(storage)?,
        name: row.try_get("name").map_err(storage)?,
        owner: PrincipalId::from_u128(identifier(row, "owner_id")?).map_err(storage)?,
        owner_email: row.try_get("owner_email").map_err(storage)?,
        active: row.try_get("active").map_err(storage)?,
        revision: number(row, "revision")?,
    })
}
fn capability(row: &PgRow) -> Result<CapabilitySummary, Error> {
    Ok(CapabilitySummary {
        id: CapabilityId::from_u128(identifier(row, "id")?).map_err(storage)?,
        key: row.try_get("name").map_err(storage)?,
        meaning: row.try_get("meaning").map_err(storage)?,
        retired: !row.try_get::<bool, _>("active").map_err(storage)?,
    })
}
fn role(row: &PgRow) -> Result<RoleSummary, Error> {
    Ok(RoleSummary {
        id: RoleId::from_u128(identifier(row, "id")?).map_err(storage)?,
        name: row.try_get("name").map_err(storage)?,
    })
}
fn resource(row: &PgRow) -> Result<ResourceRecord, Error> {
    Ok(ResourceRecord {
        id: ResourceId::from_u128(identifier(row, "id")?).map_err(storage)?,
        application: ApplicationId::from_u128(identifier(row, "application_id")?)
            .map_err(storage)?,
        name: row.try_get("name").map_err(storage)?,
        audience: row.try_get("audience").map_err(storage)?,
    })
}
fn scope(row: &PgRow) -> Result<ScopeRecord, Error> {
    Ok(ScopeRecord {
        id: ScopeId::from_u128(identifier(row, "id")?).map_err(storage)?,
        application: ApplicationId::from_u128(identifier(row, "application_id")?)
            .map_err(storage)?,
        resource: ResourceId::from_u128(identifier(row, "resource_id")?).map_err(storage)?,
        name: row.try_get("name").map_err(storage)?,
    })
}
pub(super) async fn application_exists(tx: &mut Tx<'_>, app: ApplicationId) -> Result<(), Error> {
    sqlx::query("SELECT id FROM applications WHERE id=$1")
        .bind(uuid(app.as_u128()))
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?
        .ok_or(Error::NotFound)?;
    Ok(())
}
pub(super) async fn view(tx: &mut Tx<'_>, target: Target) -> Result<View, Error> {
    let item = detail(tx, target).await?;
    let applications = bindings(tx, target).await?;
    let capabilities = grants(tx, target).await?;
    Ok(View {
        item,
        applications,
        capabilities,
        policy_revision: revision(tx).await?,
    })
}
async fn detail(tx: &mut Tx<'_>, target: Target) -> Result<Item, Error> {
    let (sql, id, app, resource, kind) = match target {
        Target::Capability(id) => (
            "SELECT id,permission_key AS name,meaning,NOT retired AS active FROM capabilities WHERE id=$1 AND $2::uuid IS NULL AND $3::uuid IS NULL",
            id.as_u128(),
            None,
            None,
            List::Capabilities(None),
        ),
        Target::Role(id) => (
            "SELECT id,name FROM roles WHERE id=$1 AND $2::uuid IS NULL AND $3::uuid IS NULL",
            id.as_u128(),
            None,
            None,
            List::Roles(None),
        ),
        Target::Resource(app, id) => (
            "SELECT * FROM protected_resources WHERE id=$1 AND application_id=$2 AND $3::uuid IS NULL",
            id.as_u128(),
            Some(app),
            None,
            List::Resources(app),
        ),
        Target::Scope(app, res, id) => (
            "SELECT * FROM resource_scopes WHERE id=$1 AND application_id=$2 AND resource_id=$3",
            id.as_u128(),
            Some(app),
            Some(res),
            List::Scopes(app),
        ),
    };
    let row = sqlx::query(sql)
        .bind(uuid(id))
        .bind(app.map(|v| uuid(v.as_u128())))
        .bind(resource.map(|v| uuid(v.as_u128())))
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?
        .ok_or(Error::NotFound)?;
    item(kind, &row)
}
async fn bindings(tx: &mut Tx<'_>, target: Target) -> Result<Vec<ApplicationRecord>, Error> {
    let (table, id_column, id) = match target {
        Target::Capability(id) => ("capability_applications", "capability_id", id.as_u128()),
        Target::Role(id) => ("role_applications", "role_id", id.as_u128()),
        _ => return Ok(vec![]),
    };
    let sql = format!(
        "{APPLICATION} JOIN {table} b ON b.application_id=a.id WHERE b.{id_column}=$1 ORDER BY a.id LIMIT 1001"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(uuid(id))
        .fetch_all(&mut **tx)
        .await
        .map_err(storage)?;
    if rows.len() > 1000 {
        return Err(Error::Unavailable);
    }
    rows.iter().map(application).collect()
}
async fn grants(tx: &mut Tx<'_>, target: Target) -> Result<Vec<CapabilitySummary>, Error> {
    let (table, key, id) = match target {
        Target::Role(id) => ("role_capabilities", "role_id", id.as_u128()),
        Target::Resource(_, id) => ("resource_capabilities", "resource_id", id.as_u128()),
        Target::Scope(_, _, id) => ("scope_capabilities", "scope_id", id.as_u128()),
        Target::Capability(_) => return Ok(vec![]),
    };
    let sql = format!(
        "SELECT c.id,c.permission_key AS name,c.meaning,NOT c.retired AS active FROM capabilities c JOIN {table} g ON g.capability_id=c.id WHERE g.{key}=$1 ORDER BY c.id LIMIT 257"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(uuid(id))
        .fetch_all(&mut **tx)
        .await
        .map_err(storage)?;
    if rows.len() > 256 {
        return Err(Error::Unavailable);
    }
    rows.iter().map(capability).collect()
}
#[cfg(test)]
#[path = "../../../tests/unit/postgres/admin_catalog/reads.rs"]
mod tests;
