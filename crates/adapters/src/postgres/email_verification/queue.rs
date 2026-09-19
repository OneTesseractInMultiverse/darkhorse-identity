use super::*;
use darkhorse_application::email_verification::{Delivery, DeliveryQueue, DeliveryResult};
impl PostgresStore {
    pub async fn bind_email_delivery(
        &self,
        origin: &str,
        fingerprint: [u8; 32],
    ) -> Result<(), Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, true).await?;
        sqlx::query("INSERT INTO email_delivery_state(origin,key_fingerprint) VALUES($1,$2) ON CONFLICT DO NOTHING").bind(origin).bind(fingerprint.as_slice()).execute(&mut *tx).await.map_err(storage)?;
        sqlx::query(
            "SELECT singleton FROM email_delivery_state WHERE origin=$1 AND key_fingerprint=$2",
        )
        .bind(origin)
        .bind(fingerprint.as_slice())
        .fetch_one(&mut *tx)
        .await
        .map_err(storage)?;
        tx.commit().await.map_err(storage)
    }
}
impl DeliveryQueue for PostgresStore {
    async fn claim_email(&self) -> Result<Option<Delivery>, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, false).await?;
        let now = now(&mut tx).await?;
        // A small bounded cleanup also removes expired or exhausted payload seeds.
        sqlx::query("UPDATE email_verifications SET seed=NULL,delivery_state='cancelled' WHERE id IN (SELECT v.id FROM email_verifications v JOIN principals p ON p.id=v.principal_id WHERE v.delivery_state='queued' AND (v.expires_ms<=$1 OR (v.attempts>=5 AND v.next_ms<=$1) OR v.consumed OR NOT p.active OR v.email<>p.email OR v.credential_epoch<>p.credential_epoch) ORDER BY v.next_ms LIMIT 100 FOR UPDATE OF v SKIP LOCKED)")
            .bind(now as i64).execute(&mut *tx).await.map_err(storage)?;
        let row=sqlx::query("SELECT v.* FROM email_verifications v JOIN principals p ON p.id=v.principal_id WHERE v.delivery_state='queued' AND NOT v.consumed AND v.attempts<5 AND v.created_ms<=$1 AND v.expires_ms>$1 AND v.next_ms<=$1 AND p.active AND p.email=v.email AND p.credential_epoch=v.credential_epoch ORDER BY v.next_ms,v.id LIMIT 1 FOR UPDATE OF v SKIP LOCKED")
            .bind(now as i64).fetch_optional(&mut *tx).await.map_err(storage)?;
        let delivery = match row {
            Some(row) => Some(claim(&mut tx, &row, now).await?),
            None => None,
        };
        tx.commit().await.map_err(storage)?;
        Ok(delivery)
    }
    async fn finish_email(
        &self,
        id: EmailVerificationId,
        attempt: u16,
        result: DeliveryResult,
    ) -> Result<(), Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, false).await?;
        let row=sqlx::query("SELECT * FROM email_verifications WHERE id=$1 AND attempts=$2 AND delivery_state='queued' FOR UPDATE")
            .bind(Uuid::from_u128(id.as_u128())).bind(attempt as i16).fetch_optional(&mut *tx).await.map_err(storage)?;
        if let Some(row) = row {
            finish(&mut tx, &row, attempt, result).await?;
        }
        tx.commit().await.map_err(storage)
    }
}
async fn claim(tx: &mut Tx<'_>, row: &PgRow, now: u64) -> Result<Delivery, Error> {
    let delivery = delivery(row)?;
    sqlx::query("UPDATE email_verifications SET attempts=attempts+1,next_ms=$2 WHERE id=$1")
        .bind(Uuid::from_u128(delivery.id.as_u128()))
        .bind(policy::lease(now, number(row, "expires_ms")?) as i64)
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    Ok(delivery)
}
fn delivery(row: &PgRow) -> Result<Delivery, Error> {
    let attempt: i16 = row.try_get("attempts").map_err(storage)?;
    Ok(Delivery {
        created_ms: number(row, "created_ms")?,
        id: id(row)?,
        attempt: (attempt + 1) as u16,
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
    let (state, event, next) = outcome(result, attempt, now, number(row, "expires_ms")?);
    sqlx::query("UPDATE email_verifications SET delivery_state=$2,seed=CASE WHEN $2='queued' THEN seed ELSE NULL END,next_ms=COALESCE($3,next_ms) WHERE id=$1")
        .bind(Uuid::from_u128(id(row)?.as_u128())).bind(state).bind(next.map(|n|n as i64)).execute(&mut **tx).await.map_err(storage)?;
    audit(tx, principal(row)?, id(row)?, None, event, attempt, now).await
}
fn outcome(
    result: DeliveryResult,
    attempt: u16,
    now: u64,
    expires: u64,
) -> (&'static str, &'static str, Option<u64>) {
    match result {
        DeliveryResult::Accepted => ("accepted", "accepted", None),
        DeliveryResult::Retry => match policy::retry(attempt, now, expires) {
            Some(next) => ("queued", "retry", Some(next)),
            None => ("failed", "failed", None),
        },
        DeliveryResult::Rejected => ("failed", "failed", None),
    }
}
