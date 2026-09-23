use super::PostgresStore;
use darkhorse_application::{
    limiting::LimiterUnavailable,
    shared_limiting::{EnforcementAuthority, RecoveryAuthority},
};
use darkhorse_domain::limiter_recovery::{
    Enforcement, Generation, ServerIdentity, activation_ready, recovery_deadline,
};
use sqlx::{Row, postgres::PgRow};
use uuid::Uuid;

pub(super) const READ: &str = "SELECT *, floor(extract(epoch FROM clock_timestamp())*1000)::bigint AS now_ms FROM limiter_authority WHERE singleton AND NOT pg_is_in_recovery()";
pub(super) const LOCK: &str = "SELECT *, floor(extract(epoch FROM clock_timestamp())*1000)::bigint AS now_ms FROM limiter_authority WHERE singleton AND NOT pg_is_in_recovery() FOR UPDATE";
fn unavailable<T>(_: T) -> LimiterUnavailable {
    LimiterUnavailable
}
pub(super) fn state(row: PgRow) -> Result<Enforcement, LimiterUnavailable> {
    let nonce: Uuid = row.try_get("generation").map_err(unavailable)?;
    let epoch: i64 = row.try_get("epoch").map_err(unavailable)?;
    let active: bool = row.try_get("active").map_err(unavailable)?;
    let identity = if active {
        Some(ServerIdentity {
            run: row
                .try_get::<Vec<u8>, _>("run_id")
                .map_err(unavailable)?
                .try_into()
                .map_err(unavailable)?,
            replication: row
                .try_get::<Vec<u8>, _>("replication_id")
                .map_err(unavailable)?
                .try_into()
                .map_err(unavailable)?,
        })
    } else {
        None
    };
    Ok(Enforcement {
        generation: Generation::new(epoch.try_into().map_err(unavailable)?, *nonce.as_bytes())
            .map_err(unavailable)?,
        active,
        identity,
        now_ms: row
            .try_get::<i64, _>("now_ms")
            .map_err(unavailable)?
            .try_into()
            .map_err(unavailable)?,
        not_before_ms: row
            .try_get::<i64, _>("not_before_ms")
            .map_err(unavailable)?
            .try_into()
            .map_err(unavailable)?,
    })
}
impl EnforcementAuthority for PostgresStore {
    async fn read(&self) -> Result<Enforcement, LimiterUnavailable> {
        state(
            sqlx::query(READ)
                .fetch_one(&self.pool)
                .await
                .map_err(unavailable)?,
        )
    }
}
impl RecoveryAuthority for PostgresStore {
    async fn fence(&self, nonce: [u8; 16]) -> Result<Enforcement, LimiterUnavailable> {
        let mut tx = self.pool.begin().await.map_err(unavailable)?;
        sqlx::query("SELECT pg_advisory_xact_lock(684028371)")
            .execute(&mut *tx)
            .await
            .map_err(unavailable)?;
        let current = sqlx::query(READ)
            .fetch_optional(&mut *tx)
            .await
            .map_err(unavailable)?
            .map(state)
            .transpose()?;
        let now: i64 =
            sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint")
                .fetch_one(&mut *tx)
                .await
                .map_err(unavailable)?;
        let (generation, deadline) = next_fence(current, nonce, now)?;
        sqlx::query("INSERT INTO limiter_authority(singleton,epoch,generation,active,not_before_ms) VALUES(true,$1,$2,false,$3) ON CONFLICT(singleton) DO UPDATE SET epoch=excluded.epoch,generation=excluded.generation,active=false,not_before_ms=excluded.not_before_ms,run_id=NULL,replication_id=NULL")
            .bind(generation.epoch() as i64).bind(Uuid::from_bytes(generation.nonce())).bind(deadline as i64).execute(&mut *tx).await.map_err(unavailable)?;
        audit(&mut tx, generation, "limiter.fenced").await?;
        let result = state(
            sqlx::query(READ)
                .fetch_one(&mut *tx)
                .await
                .map_err(unavailable)?,
        )?;
        tx.commit().await.map_err(unavailable)?;
        Ok(result)
    }
    async fn activate(
        &self,
        generation: Generation,
        identity: ServerIdentity,
    ) -> Result<(), LimiterUnavailable> {
        let mut tx = self.pool.begin().await.map_err(unavailable)?;
        activate_transaction(&mut tx, generation, identity).await?;
        tx.commit().await.map_err(unavailable)
    }
}
pub(super) async fn activate_transaction(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    generation: Generation,
    identity: ServerIdentity,
) -> Result<(), LimiterUnavailable> {
    let current = state(
        sqlx::query(LOCK)
            .fetch_one(&mut **tx)
            .await
            .map_err(unavailable)?,
    )?;
    activation_matches(current, generation)?;
    sqlx::query(
        "UPDATE limiter_authority SET active=true,run_id=$1,replication_id=$2 WHERE singleton",
    )
    .bind(identity.run.as_slice())
    .bind(identity.replication.as_slice())
    .execute(&mut **tx)
    .await
    .map_err(unavailable)?;
    audit(tx, generation, "limiter.activated").await?;
    Ok(())
}

fn activation_matches(
    state: Enforcement,
    generation: Generation,
) -> Result<(), LimiterUnavailable> {
    activation_ready(state).map_err(unavailable)?;
    if state.generation != generation {
        return Err(LimiterUnavailable);
    }
    Ok(())
}

fn next_fence(
    current: Option<Enforcement>,
    nonce: [u8; 16],
    now: i64,
) -> Result<(Generation, u64), LimiterUnavailable> {
    let epoch = current.map_or(1, |s| s.generation.epoch() + 1);
    let generation = Generation::new(epoch, nonce).map_err(unavailable)?;
    let deadline = recovery_deadline(now.try_into().map_err(unavailable)?).map_err(unavailable)?;
    if current.is_some_and(|s| s.generation.nonce() == nonce || deadline < s.not_before_ms) {
        return Err(LimiterUnavailable);
    }
    Ok((generation, deadline))
}
async fn audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    generation: Generation,
    event: &str,
) -> Result<(), LimiterUnavailable> {
    sqlx::query("INSERT INTO limiter_audit(epoch,generation,event) VALUES($1,$2,$3)")
        .bind(generation.epoch() as i64)
        .bind(Uuid::from_bytes(generation.nonce()))
        .bind(event)
        .execute(&mut **tx)
        .await
        .map_err(unavailable)?;
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/postgres/limiting.rs"]
mod tests;
