use super::*;
const USER: &str = "SELECT p.id,p.email,p.first_name,p.last_name,p.active,p.revision,p.email_verified_ms IS NOT NULL AS email_verified,EXISTS(SELECT 1 FROM platform_administrators a WHERE a.principal_id=p.id) AS administrator FROM principals p WHERE p.id=$1";
pub(super) async fn user(tx: &mut Tx<'_>, id: PrincipalId) -> Result<User, Error> {
    let row = sqlx::query(USER)
        .bind(uuid(id.as_u128()))
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?
        .ok_or(Error::NotFound)?;
    project(&row)
}
pub(super) async fn list(
    tx: &mut Tx<'_>,
    actor: PrincipalId,
    query: &Query,
) -> Result<Page, Error> {
    let pattern = prefix(&query.search);
    let rows=sqlx::query("SELECT p.id,p.email,p.first_name,p.last_name,p.active,p.revision,p.email_verified_ms IS NOT NULL AS email_verified,EXISTS(SELECT 1 FROM platform_administrators a WHERE a.principal_id=p.id) AS administrator FROM principals p WHERE ($1::boolean IS NULL OR p.active=$1) AND ($2='' OR p.email_key LIKE lower($3 COLLATE \"C\") ESCAPE '\\' OR lower(p.first_name||' '||p.last_name) LIKE lower($3) ESCAPE '\\' OR lower(p.last_name) LIKE lower($3) ESCAPE '\\') AND ($4::uuid IS NULL OR (p.created_at,p.id)>(SELECT created_at,id FROM principals WHERE id=$4)) ORDER BY p.created_at,p.id LIMIT $5")
        .bind(query.status.map(|s|s==AccountStatus::Active)).bind(&query.search).bind(pattern)
        .bind(query.after.map(|id|uuid(id.as_u128()))).bind(i64::from(query.limit)+1)
        .fetch_all(&mut **tx).await.map_err(storage)?;
    page(
        actor,
        rows.iter().map(project).collect::<Result<Vec<_>, _>>()?,
        query.limit as usize,
    )
}
fn prefix(search: &str) -> String {
    format!(
        "{}%",
        search
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )
}
fn page(actor: PrincipalId, mut items: Vec<User>, limit: usize) -> Result<Page, Error> {
    let more = items.len() > limit;
    items.truncate(limit);
    let next = more.then(|| items.last().map(|u| u.id)).flatten();
    Ok(Page { actor, items, next })
}
fn project(row: &PgRow) -> Result<User, Error> {
    Ok(User {
        id: PrincipalId::from_u128(row.try_get::<Uuid, _>("id").map_err(storage)?.as_u128())
            .map_err(storage)?,
        email: row.try_get("email").map_err(storage)?,
        first_name: row.try_get("first_name").map_err(storage)?,
        last_name: row.try_get("last_name").map_err(storage)?,
        status: if row.try_get("active").map_err(storage)? {
            AccountStatus::Active
        } else {
            AccountStatus::Inactive
        },
        revision: number(row, "revision")?,
        administrator: row.try_get("administrator").map_err(storage)?,
        email_verified: row.try_get("email_verified").map_err(storage)?,
    })
}
pub(super) async fn policy_revision(tx: &mut Tx<'_>) -> Result<u64, Error> {
    sqlx::query_scalar::<_, i64>("SELECT policy_revision FROM security_state WHERE singleton")
        .fetch_one(&mut **tx)
        .await
        .map_err(storage)?
        .try_into()
        .map_err(storage)
}
pub(super) async fn applications(tx: &mut Tx<'_>) -> Result<Vec<ApplicationSummary>, Error> {
    let rows = sqlx::query("SELECT id,name,active FROM applications ORDER BY name,id LIMIT 101")
        .fetch_all(&mut **tx)
        .await
        .map_err(storage)?;
    if rows.len() > 100 {
        return Err(Error::Unavailable);
    }
    let mut applications = Vec::new();
    for row in rows {
        let id: Uuid = row.try_get("id").map_err(storage)?;
        applications.push(ApplicationSummary {
            id: ApplicationId::from_u128(id.as_u128()).map_err(storage)?,
            name: row.try_get("name").map_err(storage)?,
            active: row.try_get("active").map_err(storage)?,
        });
    }
    Ok(applications)
}
pub(super) async fn roles(
    tx: &mut Tx<'_>,
    principal: PrincipalId,
    app: Uuid,
) -> Result<Vec<Role>, Error> {
    let rows=sqlx::query("SELECT r.id,r.name,EXISTS(SELECT 1 FROM principal_roles p WHERE p.principal_id=$1 AND p.application_id=$2 AND p.role_id=r.id) AS assigned FROM roles r JOIN role_applications a ON a.role_id=r.id WHERE a.application_id=$2 ORDER BY r.name,r.id LIMIT 129")
        .bind(uuid(principal.as_u128())).bind(app).fetch_all(&mut **tx).await.map_err(storage)?;
    if rows.len() > 128 {
        return Err(Error::Unavailable);
    }
    rows.iter()
        .map(|row| {
            Ok(Role {
                id: RoleId::from_u128(row.try_get::<Uuid, _>("id").map_err(storage)?.as_u128())
                    .map_err(storage)?,
                name: row.try_get("name").map_err(storage)?,
                assigned: row.try_get("assigned").map_err(storage)?,
            })
        })
        .collect()
}
#[cfg(test)]
#[path = "../../../tests/unit/postgres/admin_directory/reads.rs"]
mod tests;

pub(super) fn selection(
    applications: &[ApplicationSummary],
    requested: Option<ApplicationId>,
) -> Result<Option<ApplicationId>, Error> {
    if let Some(id) = requested {
        if !applications.iter().any(|app| app.id == id) {
            return Err(Error::NotFound);
        }
        return Ok(Some(id));
    }
    Ok(applications.first().map(|app| app.id))
}
