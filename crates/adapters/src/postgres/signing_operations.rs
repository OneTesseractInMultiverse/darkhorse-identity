use super::{PostgresStore, signing};
use darkhorse_application::{signing::WrappedKey, signing_operations::*};
use darkhorse_domain::{
    identity::OperationId,
    signing::{KeyError, Phase},
};
use sqlx::{Row, postgres::PgRow};
use uuid::Uuid;
fn unavailable<T>(_: T) -> Error {
    Error::Rejected(KeyError::Unavailable)
}
impl Journal for PostgresStore {
    async fn prepare_signing(&self, intent: &Intent) -> Result<(), Error> {
        let mut tx = self.pool.begin().await.map_err(unavailable)?;
        sqlx::query("INSERT INTO signing_operation_intents(operation_id,operation,issuer,kid,expected_revision) VALUES($1,$2,$3,$4,$5)")
            .bind(Uuid::from_u128(intent.id.as_u128())).bind(kind_name(intent.kind)).bind(&intent.issuer).bind(&intent.kid).bind(integer(intent.expected_revision)?)
            .execute(&mut *tx).await.map_err(unavailable)?;
        tx.commit().await.map_err(|_| Error::Uncertain)
    }
    async fn complete_signing(
        &self,
        intent: &Intent,
        wrap_digest: [u8; 32],
        key: Option<WrappedKey>,
    ) -> Result<u64, Error> {
        validate_material(intent, key.as_ref())?;
        let mut tx = self.pool.begin().await.map_err(unavailable)?;
        let recorded:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM signing_operation_intents i WHERE operation_id=$1 AND operation=$2 AND issuer=$3 AND kid=$4 AND expected_revision=$5 AND database_role=session_user AND prepared_ms<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND NOT EXISTS(SELECT 1 FROM signing_operation_receipts r WHERE r.operation_id=i.operation_id))")
            .bind(Uuid::from_u128(intent.id.as_u128())).bind(kind_name(intent.kind)).bind(&intent.issuer).bind(&intent.kid).bind(integer(intent.expected_revision)?)
            .fetch_one(&mut *tx).await.map_err(unavailable)?;
        if !recorded {
            return Err(KeyError::Conflict.into());
        }
        signing::bind_transaction(&mut tx, &intent.issuer, wrap_digest).await?;
        let result = mutate(&mut tx, intent, wrap_digest, key).await?;
        sqlx::query("INSERT INTO signing_operation_receipts(operation_id,audit_id) VALUES($1,$2)")
            .bind(Uuid::from_u128(intent.id.as_u128()))
            .bind(result.audit_id)
            .execute(&mut *tx)
            .await
            .map_err(unavailable)?;
        tx.commit().await.map_err(|_| Error::Uncertain)?;
        Ok(result.revision)
    }
    async fn inspect_signing(&self, id: OperationId) -> Result<Attempt, Error> {
        // One primary snapshot. A missing receipt does not rule out an in-flight commit.
        let row=sqlx::query("SELECT pg_is_in_recovery() AS in_recovery,i.*,r.completed_ms,a.revision AS completed_revision,p.revision AS current_revision,k.phase AS current_phase,floor(extract(epoch FROM clock_timestamp())*1000)::bigint AS now_ms FROM (SELECT 1) probe LEFT JOIN signing_operation_intents i ON i.operation_id=$1 LEFT JOIN signing_operation_receipts r USING(operation_id) LEFT JOIN provider_audit a ON a.id=r.audit_id LEFT JOIN provider_state p ON p.singleton AND p.issuer=i.issuer LEFT JOIN signing_keys k ON p.singleton AND k.kid=i.kid")
            .bind(Uuid::from_u128(id.as_u128())).fetch_one(&self.pool).await.map_err(unavailable)?;
        project(id, row)
    }
}
fn validate_material(intent: &Intent, key: Option<&WrappedKey>) -> Result<(), Error> {
    match (intent.kind, key) {
        (Kind::Generate | Kind::Import, Some(key)) if key.public.kid == intent.kid => Ok(()),
        (Kind::Activate | Kind::Retire, None) => Ok(()),
        _ => Err(KeyError::Invalid.into()),
    }
}
async fn mutate(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    intent: &Intent,
    wrap: [u8; 32],
    key: Option<WrappedKey>,
) -> Result<signing::Mutation, Error> {
    let result = match (intent.kind, key) {
        (Kind::Generate | Kind::Import, Some(key)) => {
            signing::stage_transaction(tx, &intent.issuer, wrap, intent.expected_revision, key)
                .await
        }
        (Kind::Activate, None) => {
            signing::activate_transaction(tx, &intent.issuer, &intent.kid, intent.expected_revision)
                .await
        }
        (Kind::Retire, None) => {
            signing::retire_transaction(tx, &intent.issuer, &intent.kid, intent.expected_revision)
                .await
        }
        _ => return Err(KeyError::Invalid.into()),
    };
    result.map_err(Error::from)
}
fn integer(value: u64) -> Result<i64, Error> {
    value.try_into().map_err(|_| KeyError::Invalid.into())
}
fn time(value: i64) -> Result<u64, Error> {
    value.try_into().map_err(unavailable)
}
fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Generate => "generate",
        Kind::Import => "import",
        Kind::Activate => "activate",
        Kind::Retire => "retire",
    }
}
fn parse_kind(value: &str) -> Result<Kind, Error> {
    match value {
        "generate" => Ok(Kind::Generate),
        "import" => Ok(Kind::Import),
        "activate" => Ok(Kind::Activate),
        "retire" => Ok(Kind::Retire),
        _ => Err(KeyError::Unavailable.into()),
    }
}
fn parse_phase(value: &str) -> Result<Phase, Error> {
    match value {
        "staged" => Ok(Phase::Staged),
        "active" => Ok(Phase::Active),
        "retiring" => Ok(Phase::Retiring),
        "retired" => Ok(Phase::Retired),
        _ => Err(KeyError::Unavailable.into()),
    }
}
fn project(id: OperationId, row: PgRow) -> Result<Attempt, Error> {
    if row.try_get::<bool, _>("in_recovery").map_err(unavailable)? {
        return Err(KeyError::Unavailable.into());
    }
    if row
        .try_get::<Option<Uuid>, _>("operation_id")
        .map_err(unavailable)?
        .is_none()
    {
        return Err(KeyError::NotFound.into());
    }
    let completion = row
        .try_get::<Option<i64>, _>("completed_ms")
        .map_err(unavailable)?
        .map(|ms| {
            Ok::<_, Error>(Completion {
                completed_ms: time(ms)?,
                revision: time(row.try_get("completed_revision").map_err(unavailable)?)?,
            })
        })
        .transpose()?;
    Ok(Attempt {
        intent: Intent {
            id,
            issuer: row.try_get("issuer").map_err(unavailable)?,
            kid: row.try_get("kid").map_err(unavailable)?,
            kind: parse_kind(row.try_get("operation").map_err(unavailable)?)?,
            expected_revision: time(row.try_get("expected_revision").map_err(unavailable)?)?,
        },
        database_role: row.try_get("database_role").map_err(unavailable)?,
        prepared_ms: time(row.try_get("prepared_ms").map_err(unavailable)?)?,
        completion,
        current_revision: row
            .try_get::<Option<i64>, _>("current_revision")
            .map_err(unavailable)?
            .map(time)
            .transpose()?,
        current_phase: row
            .try_get::<Option<&str>, _>("current_phase")
            .map_err(unavailable)?
            .map(parse_phase)
            .transpose()?,
        database_ms: time(row.try_get("now_ms").map_err(unavailable)?)?,
    })
}
#[cfg(test)]
#[path = "../../tests/unit/postgres/signing_operations.rs"]
mod tests;
