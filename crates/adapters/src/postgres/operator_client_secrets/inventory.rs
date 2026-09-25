use super::*;
use sqlx::{Row, postgres::PgRow};
pub(super) async fn read(tx: &mut Tx<'_>, request: &Request) -> Result<Inventory, Error> {
    let Operation::List { after, limit } = request.operation() else {
        return Err(Error::Invalid);
    };
    let target = request.target();
    let revision: i64 =
        sqlx::query_scalar("SELECT revision FROM oauth_clients WHERE id=$1 AND application_id=$2")
            .bind(Uuid::from_u128(target.client.as_u128()))
            .bind(Uuid::from_u128(target.application.as_u128()))
            .fetch_optional(&mut **tx)
            .await
            .map_err(storage)?
            .ok_or(Error::NotFound)?;
    let now = sessions::now(tx).await.map_err(storage)?;
    let rows=sqlx::query("SELECT id,created_ms,expires_ms,retired FROM oauth_client_secrets WHERE client_id=$1 AND ($2::uuid IS NULL OR id>$2) ORDER BY id LIMIT $3")
  .bind(Uuid::from_u128(target.client.as_u128())).bind(after.map(|id|Uuid::from_u128(id.as_u128()))).bind(i64::from(limit)+1).fetch_all(&mut **tx).await.map_err(storage)?;
    let items = rows.iter().map(metadata).collect::<Result<Vec<_>, _>>()?;
    page(revision, now, items, limit)
}
fn metadata(row: &PgRow) -> Result<Metadata, Error> {
    Ok(Metadata {
        id: ClientSecretId::from_u128(row.try_get::<Uuid, _>("id").map_err(storage)?.as_u128())
            .map_err(storage)?,
        created_ms: u64::try_from(row.try_get::<i64, _>("created_ms").map_err(storage)?)
            .map_err(storage)?,
        expires_ms: row
            .try_get::<Option<i64>, _>("expires_ms")
            .map_err(storage)?
            .map(u64::try_from)
            .transpose()
            .map_err(storage)?,
        retired: row.try_get("retired").map_err(storage)?,
    })
}
fn page(
    revision: i64,
    observed_ms: u64,
    mut items: Vec<Metadata>,
    limit: u16,
) -> Result<Inventory, Error> {
    if !(1..=25).contains(&limit) || items.len() > usize::from(limit) + 1 {
        return Err(Error::Unavailable);
    }
    let more = items.len() > usize::from(limit);
    items.truncate(usize::from(limit));
    let next = if more {
        items.last().map(|item| item.id)
    } else {
        None
    };
    Ok(Inventory {
        revision: revision.try_into().map_err(storage)?,
        observed_ms,
        items,
        next,
    })
}
#[cfg(test)]
#[path = "../../../tests/unit/postgres/operator_client_secrets/inventory.rs"]
mod tests;
