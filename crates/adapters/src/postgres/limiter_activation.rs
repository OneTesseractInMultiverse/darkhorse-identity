use super::{PostgresStore, limiting};
use darkhorse_application::limiter_activation::{Attempt, Completion, Error, Journal};
use darkhorse_domain::{
    identity::OperationId,
    limiter_recovery::{Enforcement, Generation, ServerIdentity, activation_ready},
};
use sqlx::{Row, postgres::PgRow};
use uuid::Uuid;
fn unavailable<T>(_: T) -> Error {
    Error::Unavailable
}
impl Journal for PostgresStore {
    async fn prepare(&self, id: OperationId) -> Result<Enforcement, Error> {
        let mut tx = self.pool.begin().await.map_err(unavailable)?;
        let state = limiting::state(
            sqlx::query(limiting::LOCK)
                .fetch_one(&mut *tx)
                .await
                .map_err(unavailable)?,
        )
        .map_err(unavailable)?;
        activation_ready(state).map_err(|_| Error::NotReady)?;
        sqlx::query("INSERT INTO limiter_activation_intents(operation_id,epoch,generation,not_before_ms) VALUES($1,$2,$3,$4)")
   .bind(Uuid::from_u128(id.as_u128())).bind(state.generation.epoch() as i64).bind(Uuid::from_bytes(state.generation.nonce())).bind(state.not_before_ms as i64)
   .execute(&mut *tx).await.map_err(unavailable)?;
        tx.commit().await.map_err(|_| Error::Uncertain)?;
        Ok(state)
    }
    async fn complete(
        &self,
        id: OperationId,
        generation: Generation,
        identity: ServerIdentity,
    ) -> Result<(), Error> {
        let mut tx = self.pool.begin().await.map_err(unavailable)?;
        let recorded:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM limiter_activation_intents WHERE operation_id=$1 AND epoch=$2 AND generation=$3 AND database_role=session_user AND prepared_ms<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint)")
   .bind(Uuid::from_u128(id.as_u128())).bind(generation.epoch() as i64).bind(Uuid::from_bytes(generation.nonce())).fetch_one(&mut *tx).await.map_err(unavailable)?;
        if !recorded {
            return Err(Error::Unavailable);
        }
        limiting::activate_transaction(&mut tx, generation, identity)
            .await
            .map_err(unavailable)?;
        sqlx::query("INSERT INTO limiter_activation_receipts(operation_id,run_id,replication_id) VALUES($1,$2,$3)")
   .bind(Uuid::from_u128(id.as_u128())).bind(identity.run.as_slice()).bind(identity.replication.as_slice()).execute(&mut *tx).await.map_err(unavailable)?;
        tx.commit().await.map_err(|_| Error::Uncertain)
    }
    async fn inspect(&self, id: OperationId) -> Result<Attempt, Error> {
        // One primary snapshot: completed is historical evidence, not current Redis health.
        let row=sqlx::query("SELECT p.in_recovery,a.*,r.completed_ms,r.run_id AS completed_run,r.replication_id AS completed_replication,l.epoch AS current_epoch,l.generation AS current_generation,l.active,l.not_before_ms AS current_not_before,l.run_id,l.replication_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint AS now_ms FROM (SELECT pg_is_in_recovery() AS in_recovery) p LEFT JOIN limiter_activation_intents a ON a.operation_id=$1 LEFT JOIN limiter_activation_receipts r USING(operation_id) LEFT JOIN limiter_authority l ON l.singleton")
   .bind(Uuid::from_u128(id.as_u128())).fetch_one(&self.pool).await.map_err(unavailable)?;
        attempt(id, row)
    }
}
fn generation(epoch: i64, nonce: Uuid) -> Result<Generation, Error> {
    Generation::new(epoch.try_into().map_err(unavailable)?, *nonce.as_bytes()).map_err(unavailable)
}
fn bytes(value: Vec<u8>) -> Result<[u8; 20], Error> {
    value.try_into().map_err(unavailable)
}
fn time(value: i64) -> Result<u64, Error> {
    value.try_into().map_err(unavailable)
}
fn attempt(id: OperationId, row: PgRow) -> Result<Attempt, Error> {
    if row.try_get::<bool, _>("in_recovery").map_err(unavailable)? {
        return Err(Error::Unavailable);
    }
    if row
        .try_get::<Option<Uuid>, _>("operation_id")
        .map_err(unavailable)?
        .is_none()
    {
        return Err(Error::NotFound);
    }
    let active: bool = row.try_get("active").map_err(unavailable)?;
    let current = Enforcement {
        generation: generation(
            row.try_get("current_epoch").map_err(unavailable)?,
            row.try_get("current_generation").map_err(unavailable)?,
        )?,
        active,
        not_before_ms: time(row.try_get("current_not_before").map_err(unavailable)?)?,
        now_ms: time(row.try_get("now_ms").map_err(unavailable)?)?,
        identity: if active {
            Some(ServerIdentity {
                run: bytes(row.try_get("run_id").map_err(unavailable)?)?,
                replication: bytes(row.try_get("replication_id").map_err(unavailable)?)?,
            })
        } else {
            None
        },
    };
    let completed: Option<i64> = row.try_get("completed_ms").map_err(unavailable)?;
    let completion = completed
        .map(|ms| {
            Ok(Completion {
                completed_ms: time(ms)?,
                identity: ServerIdentity {
                    run: bytes(row.try_get("completed_run").map_err(unavailable)?)?,
                    replication: bytes(row.try_get("completed_replication").map_err(unavailable)?)?,
                },
            })
        })
        .transpose()?;
    Ok(Attempt {
        id,
        generation: generation(
            row.try_get("epoch").map_err(unavailable)?,
            row.try_get("generation").map_err(unavailable)?,
        )?,
        not_before_ms: time(row.try_get("not_before_ms").map_err(unavailable)?)?,
        prepared_ms: time(row.try_get("prepared_ms").map_err(unavailable)?)?,
        database_role: row.try_get("database_role").map_err(unavailable)?,
        completion,
        current,
    })
}
