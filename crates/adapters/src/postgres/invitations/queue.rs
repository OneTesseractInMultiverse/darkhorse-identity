use super::*;
use darkhorse_application::email_delivery::DeliveryResult;
use darkhorse_domain::email_delivery as delivery_policy;
impl InvitationQueue for PostgresStore {
    async fn claim_invitation(&self) -> Result<Option<Delivery>, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, false).await?;
        let now = now(&mut tx).await?;
        cleanup(&mut tx, now).await?;
        let row = sqlx::query("SELECT i.* FROM invitations i JOIN principals p ON p.id=i.issuer_id JOIN credentials c ON c.id=i.issuer_credential_id AND c.principal_id=p.id JOIN password_credentials pc ON pc.credential_id=c.id WHERE i.delivery_state='queued' AND NOT i.closed AND i.attempts<5 AND i.created_ms<=$1 AND i.expires_ms>$1 AND i.next_ms<=$1 AND p.active AND p.credential_epoch=i.issuer_epoch AND NOT c.revoked AND EXISTS(SELECT 1 FROM platform_administrators a WHERE a.principal_id=p.id) AND NOT EXISTS(SELECT 1 FROM principals recipient WHERE recipient.email_key=i.email_key) ORDER BY i.next_ms,i.id LIMIT 1 FOR UPDATE OF i SKIP LOCKED")
    .bind(now as i64)
        .fetch_optional(&mut *tx).await.map_err(storage)?;
        let delivery = match row {
            Some(row) => Some(claim(&mut tx, &row, now).await?),
            None => None,
        };
        tx.commit().await.map_err(storage)?;
        Ok(delivery)
    }
    async fn finish_invitation(
        &self,
        id: InvitationId,
        attempt: u16,
        result: DeliveryResult,
    ) -> Result<(), Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, false).await?;
        let row = sqlx::query("SELECT * FROM invitations WHERE id=$1 AND attempts=$2 AND delivery_state='queued' FOR UPDATE")
    .bind(uuid(id.as_u128()))
        .bind(attempt as i16)
        .fetch_optional(&mut *tx).await.map_err(storage)?;
        if let Some(row) = row {
            finish(&mut tx, &row, attempt, result).await?;
        }
        tx.commit().await.map_err(storage)
    }
}
async fn cleanup(tx: &mut Tx<'_>, now: u64) -> Result<(), Error> {
    sqlx::query("UPDATE invitations SET seed=NULL,delivery_state='cancelled' WHERE id IN (SELECT i.id FROM invitations i JOIN principals p ON p.id=i.issuer_id JOIN credentials c ON c.id=i.issuer_credential_id WHERE i.delivery_state='queued' AND (i.expires_ms<=$1 OR (i.attempts>=5 AND i.next_ms<=$1) OR i.closed OR NOT p.active OR p.credential_epoch<>i.issuer_epoch OR c.revoked OR NOT EXISTS(SELECT 1 FROM platform_administrators a WHERE a.principal_id=p.id) OR EXISTS(SELECT 1 FROM principals recipient WHERE recipient.email_key=i.email_key)) ORDER BY i.next_ms LIMIT 100 FOR UPDATE OF i SKIP LOCKED)")
 .bind(now as i64)
        .execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
async fn claim(tx: &mut Tx<'_>, row: &PgRow, now: u64) -> Result<Delivery, Error> {
    let delivery = delivery(row)?;
    sqlx::query("UPDATE invitations SET attempts=attempts+1,next_ms=$2 WHERE id=$1")
        .bind(uuid(delivery.id.as_u128()))
        .bind(delivery_policy::lease(now, number(row, "expires_ms")?) as i64)
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    Ok(delivery)
}
fn delivery(row: &PgRow) -> Result<Delivery, Error> {
    Ok(Delivery {
        id: id(row)?,
        created_ms: number(row, "created_ms")?,
        attempt: (row.try_get::<i16, _>("attempts").map_err(storage)? + 1) as u16,
        email: row.try_get("email").map_err(storage)?,
        seed: row
            .try_get::<Vec<u8>, _>("seed")
            .map_err(storage)?
            .try_into()
            .map_err(storage)?,
    })
}
async fn finish(
    tx: &mut Tx<'_>,
    row: &PgRow,
    attempt: u16,
    result: DeliveryResult,
) -> Result<(), Error> {
    let now = now(tx).await?;
    let (state, event, next) =
        super::super::email_queue::outcome(result, attempt, now, number(row, "expires_ms")?);
    sqlx::query("UPDATE invitations SET delivery_state=$2,seed=CASE WHEN $2='queued' THEN seed ELSE NULL END,next_ms=COALESCE($3,next_ms) WHERE id=$1")
 .bind(uuid(id(row)?.as_u128()))
        .bind(state)
        .bind(next.map(|n|n as i64))
        .execute(&mut **tx).await.map_err(storage)?;
    audit(tx, id(row)?, None, event, attempt, now).await
}
