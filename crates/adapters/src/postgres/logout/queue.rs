use super::*;
use darkhorse_application::logout::{
    LogoutClaims,
    delivery::{Delivery, Queue},
};
use darkhorse_domain::{
    identity::*,
    logout::{self, DeliveryResult, Outcome},
    tokens::Error,
};

impl Queue for PostgresStore {
    async fn claim(&self) -> Result<Option<Delivery>, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        crate::postgres::sessions::lock(&mut tx, false)
            .await
            .map_err(storage)?;
        let now = crate::postgres::sessions::now(&mut tx)
            .await
            .map_err(storage)?;
        maintenance::expand(&mut tx).await.map_err(storage)?;
        maintenance::cleanup(&mut tx, now as i64)
            .await
            .map_err(storage)?;
        let row = sqlx::query("SELECT d.*,r.issuer,r.principal_id,v.uri, c.active AND a.active AND v.revision=(SELECT max(revision) FROM logout_destinations WHERE client_id=d.client_id) AS enabled FROM logout_deliveries d JOIN relying_party_sessions r ON r.sid=d.sid JOIN logout_destinations v ON v.client_id=d.client_id AND v.revision=d.revision JOIN oauth_clients c ON c.id=d.client_id JOIN applications a ON a.id=c.application_id WHERE d.state='queued' AND d.next_ms<=$1 ORDER BY d.next_ms,d.id FOR UPDATE OF d SKIP LOCKED LIMIT 1")
            .bind(now as i64).fetch_optional(&mut *tx).await.map_err(storage)?;
        let delivery = match row {
            Some(row) => prepare(&mut tx, row, now).await?,
            None => None,
        };
        tx.commit().await.map_err(storage)?;
        Ok(delivery)
    }
    async fn finish(
        &self,
        id: LogoutDeliveryId,
        attempt: u16,
        result: DeliveryResult,
    ) -> Result<(), Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        crate::postgres::sessions::lock(&mut tx, false)
            .await
            .map_err(storage)?;
        let now = crate::postgres::sessions::now(&mut tx)
            .await
            .map_err(storage)?;
        let row = sqlx::query("SELECT * FROM logout_deliveries WHERE id=$1 AND state='queued' AND attempts=$2 FOR UPDATE")
            .bind(Uuid::from_u128(id.as_u128())).bind(attempt as i16).fetch_optional(&mut *tx).await.map_err(storage)?;
        if let Some(row) = row {
            let created = number(&row, "created_ms")?;
            let (state, event, next) =
                completion(logout::outcome(result, attempt, created, now), now);
            update(&mut tx, &row, state, event, next).await?;
        }
        tx.commit().await.map_err(storage)
    }
}
async fn prepare(tx: &mut Tx<'_>, row: PgRow, now: u64) -> Result<Option<Delivery>, Error> {
    let created = number(&row, "created_ms")?;
    let attempts = row
        .try_get::<i16, _>("attempts")
        .map_err(storage)?
        .try_into()
        .map_err(storage)?;
    let enabled: bool = row.try_get("enabled").map_err(storage)?;
    if !enabled {
        update(tx, &row, "cancelled", "cancelled", now).await?;
        return Ok(None);
    }
    let Some((attempt, lease)) = logout::claim(attempts, created, now) else {
        update(tx, &row, "failed", "failed", now).await?;
        return Ok(None);
    };
    let issuer: String = row.try_get("issuer").map_err(storage)?;
    let key = crate::postgres::tokens::logout_key(tx, &issuer).await?;
    let token = LogoutTokenId::from_u128(uuid::Uuid::new_v4().as_u128()).map_err(storage)?;
    let delivery = decode(&row, issuer, key, attempt, now, token)?;
    let changed =
        sqlx::query("UPDATE logout_deliveries SET attempts=attempts+1,next_ms=$2 WHERE id=$1")
            .bind(Uuid::from_u128(delivery.id.as_u128()))
            .bind(lease as i64)
            .execute(&mut **tx)
            .await
            .map_err(storage)?;
    one(changed.rows_affected()).map_err(storage)?;
    audit(
        tx,
        uuid(&row, "client_id")?,
        Some(Uuid::from_u128(delivery.id.as_u128())),
        row.try_get("revision").map_err(storage)?,
        "claimed",
        now as i64,
        None,
    )
    .await
    .map_err(storage)?;
    Ok(Some(delivery))
}
fn decode(
    row: &PgRow,
    issuer: String,
    key: darkhorse_application::signing::WrappedKey,
    attempt: u16,
    now: u64,
    token: LogoutTokenId,
) -> Result<Delivery, Error> {
    Ok(Delivery {
        id: LogoutDeliveryId::from_u128(uuid(row, "id")?.as_u128()).map_err(storage)?,
        attempt,
        destination: row.try_get("uri").map_err(storage)?,
        key,
        claims: LogoutClaims {
            issuer,
            client: ClientId::from_u128(uuid(row, "client_id")?.as_u128()).map_err(storage)?,
            subject: PrincipalId::from_u128(uuid(row, "principal_id")?.as_u128())
                .map_err(storage)?,
            session: RelyingPartySessionId::from_u128(uuid(row, "sid")?.as_u128())
                .map_err(storage)?,
            token,
            issued: now / 1000,
        },
    })
}
fn completion(result: Outcome, now: u64) -> (&'static str, &'static str, u64) {
    match result {
        Outcome::Delivered => ("delivered", "delivered", now),
        Outcome::Failed => ("failed", "failed", now),
        Outcome::RetryAt(next) => ("queued", "retry", next),
    }
}
async fn update(
    tx: &mut Tx<'_>,
    row: &PgRow,
    state: &str,
    event: &str,
    next: u64,
) -> Result<(), Error> {
    let changed = sqlx::query("UPDATE logout_deliveries SET state=$2,next_ms=$3 WHERE id=$1")
        .bind(uuid(row, "id")?)
        .bind(state)
        .bind(next as i64)
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    let now = crate::postgres::sessions::now(tx).await.map_err(storage)?;
    one(changed.rows_affected()).map_err(storage)?;
    audit(
        tx,
        uuid(row, "client_id")?,
        Some(uuid(row, "id")?),
        row.try_get("revision").map_err(storage)?,
        event,
        now as i64,
        None,
    )
    .await
    .map_err(storage)
}
fn uuid(row: &PgRow, column: &str) -> Result<Uuid, Error> {
    row.try_get(column).map_err(storage)
}
fn number(row: &PgRow, column: &str) -> Result<u64, Error> {
    row.try_get::<i64, _>(column)
        .map_err(storage)?
        .try_into()
        .map_err(storage)
}
fn storage(_: impl std::fmt::Debug) -> Error {
    Error::Unavailable
}
