use super::*;
use darkhorse_domain::oidc::{Request, Session};

pub(super) async fn revision(tx: &mut Tx<'_>, handle: [u8; 32]) -> Result<(), Error> {
    let unchanged: bool = sqlx::query_scalar("SELECT r.resource_policy_revision=s.policy_revision FROM authorization_requests r CROSS JOIN security_state s WHERE r.digest=$1 AND s.singleton")
        .bind(handle.as_slice()).fetch_one(&mut **tx).await.map_err(storage)?;
    require_unchanged(unchanged)
}
fn require_unchanged(unchanged: bool) -> Result<(), Error> {
    if unchanged {
        Ok(())
    } else {
        Err(Error::InvalidGrant)
    }
}
pub(in crate::postgres) async fn approve(
    tx: &mut Tx<'_>,
    handle: [u8; 32],
    request: &Request,
    session: Session,
) -> Result<Vec<Uuid>, Error> {
    let Some(audience) = request.resource.as_deref() else {
        return Ok(Vec::new());
    };
    revision(tx, handle).await?;
    let grant = plan(
        tx,
        session.principal,
        request.client,
        audience,
        &request.scopes,
        None,
    )
    .await?;
    let ceiling = encoded(&grant.ceiling);
    sqlx::query("INSERT INTO authorization_resource_grants(request_digest,resource_id,principal_epoch,capability_ceiling) VALUES($1,$2,$3,$4)")
        .bind(handle.as_slice()).bind(uuid(grant.target.resource.as_u128())).bind(grant.principal_epoch as i64).bind(&ceiling).execute(&mut **tx).await.map_err(storage)?;
    Ok(ceiling)
}
pub(in crate::postgres) async fn approved(
    tx: &mut Tx<'_>,
    handle: [u8; 32],
    request: &Request,
    session: Session,
) -> Result<Option<IssuancePlan>, Error> {
    let Some(audience) = request.resource.as_deref() else {
        return Ok(None);
    };
    let row=sqlx::query("SELECT resource_id,principal_epoch,capability_ceiling FROM authorization_resource_grants WHERE request_digest=$1")
        .bind(handle.as_slice()).fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(Error::InvalidGrant)?;
    let ceiling = capabilities(
        &row.try_get::<Vec<Uuid>, _>("capability_ceiling")
            .map_err(storage)?,
    )?;
    let grant = plan(
        tx,
        session.principal,
        request.client,
        audience,
        &request.scopes,
        Some(&ceiling),
    )
    .await?;
    validate_binding(&row, &grant)?;
    Ok(Some(grant))
}
fn validate_binding(row: &PgRow, grant: &IssuancePlan) -> Result<(), Error> {
    let epoch: i64 = row.try_get("principal_epoch").map_err(storage)?;
    if id(row, "resource_id")? != grant.target.resource.as_u128()
        || u64::try_from(epoch).map_err(storage)? != grant.principal_epoch
    {
        return Err(Error::InvalidGrant);
    }
    Ok(())
}
