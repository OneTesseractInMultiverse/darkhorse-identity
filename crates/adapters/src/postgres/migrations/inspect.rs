use super::{Error, OperationId, authority, identifier, plan::LIMIT};
use sqlx::PgPool;
#[derive(Debug)]
pub struct Inspection {
    pub database_role: String,
    pub prepared_ms: i64,
    pub completed_ms: Option<i64>,
    pub database_ms: i64,
    pub steps: Vec<Step>,
}
#[derive(Debug, sqlx::FromRow)]
pub struct Step {
    pub version: i64,
    pub checksum: String,
    pub already_applied: bool,
    pub completed_ms: Option<i64>,
    pub current_matches: bool,
}
#[derive(sqlx::FromRow)]
struct Record {
    database_role: String,
    prepared_ms: i64,
    manifest_count: i32,
    completed_ms: Option<i64>,
    database_ms: i64,
}
pub(super) async fn inspect(pool: &PgPool, id: OperationId) -> Result<Option<Inspection>, Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *transaction)
        .await?;
    authority(&mut transaction).await?;
    let exists: bool =
        sqlx::query_scalar("SELECT to_regnamespace('darkhorse_migration_v1') IS NOT NULL")
            .fetch_one(&mut *transaction)
            .await?;
    if !exists {
        return Ok(None);
    }
    let record = sqlx::query_as::<_,Record>("SELECT i.database_role,i.prepared_ms,i.manifest_count,c.completed_ms,floor(extract(epoch FROM clock_timestamp())*1000)::bigint AS database_ms FROM darkhorse_migration_v1.intents i LEFT JOIN darkhorse_migration_v1.completions c USING(operation_id) WHERE i.operation_id=$1")
        .bind(identifier(id)).fetch_optional(&mut *transaction).await?;
    let Some(record) = record else {
        return Ok(None);
    };
    let steps = sqlx::query_as::<_,Step>("SELECT t.version,encode(t.checksum,'hex') AS checksum,t.already_applied,s.completed_ms,COALESCE(m.success AND m.checksum=t.checksum,false) AS current_matches FROM darkhorse_migration_v1.targets t LEFT JOIN darkhorse_migration_v1.steps s USING(operation_id,version) LEFT JOIN public._sqlx_migrations m ON m.version=t.version WHERE t.operation_id=$1 ORDER BY t.version LIMIT 129")
        .bind(identifier(id)).fetch_all(&mut *transaction).await?;
    project(record, steps).map(Some)
}
fn project(record: Record, steps: Vec<Step>) -> Result<Inspection, Error> {
    if steps.is_empty()
        || steps.len() > LIMIT
        || usize::try_from(record.manifest_count).ok() != Some(steps.len())
    {
        return Err(Error::Incompatible);
    }
    Ok(Inspection {
        database_role: record.database_role,
        prepared_ms: record.prepared_ms,
        completed_ms: record.completed_ms,
        database_ms: record.database_ms,
        steps,
    })
}
#[cfg(test)]
#[path = "../../../tests/unit/postgres/migrations/inspect.rs"]
mod tests;
