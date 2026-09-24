use super::{
    Error, OperationId, authority, identifier,
    plan::{self, Applied},
};
use sqlx::{
    Connection, PgConnection,
    migrate::{Migrate, Migration},
};

pub(super) async fn prepare(
    connection: &mut PgConnection,
    id: OperationId,
    manifest: &[Migration],
) -> Result<usize, Error> {
    let mut transaction = connection.begin().await?;
    transaction
        .ensure_migrations_table("public._sqlx_migrations")
        .await?;
    let rows: Vec<(i64, Vec<u8>, bool)> = sqlx::query_as(
        "SELECT version,checksum,success FROM public._sqlx_migrations ORDER BY version LIMIT 129",
    )
    .fetch_all(&mut *transaction)
    .await?;
    let applied = rows
        .into_iter()
        .map(|(version, checksum, success)| Applied {
            version,
            checksum,
            success,
        })
        .collect::<Vec<_>>();
    let baseline = plan::baseline(manifest, &applied)?;
    initialize(&mut transaction).await?;
    insert_intent(&mut transaction, id, manifest, baseline).await?;
    transaction.commit().await.map_err(|_| Error::Uncertain)?;
    Ok(baseline)
}
async fn initialize(connection: &mut PgConnection) -> Result<(), Error> {
    let exists: bool =
        sqlx::query_scalar("SELECT to_regnamespace('darkhorse_migration_v1') IS NOT NULL")
            .fetch_one(&mut *connection)
            .await?;
    if !exists {
        sqlx::raw_sql(include_str!("journal-v1.sql"))
            .execute(connection)
            .await?;
    }
    Ok(())
}
async fn insert_intent(
    connection: &mut PgConnection,
    id: OperationId,
    manifest: &[Migration],
    baseline: usize,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO darkhorse_migration_v1.intents(operation_id,manifest_count,baseline_count) VALUES ($1,$2,$3)")
        .bind(identifier(id)).bind(i32::try_from(manifest.len()).map_err(|_|Error::Incompatible)?).bind(i32::try_from(baseline).map_err(|_|Error::Incompatible)?)
        .execute(&mut *connection).await?;
    for (index, migration) in manifest.iter().enumerate() {
        sqlx::query("INSERT INTO darkhorse_migration_v1.targets(operation_id,version,checksum,already_applied) VALUES ($1,$2,$3,$4)")
            .bind(identifier(id)).bind(migration.version).bind(migration.checksum.as_ref()).bind(index<baseline).execute(&mut *connection).await?;
    }
    Ok(())
}
pub(super) async fn complete(connection: &mut PgConnection, id: OperationId) -> Result<(), Error> {
    let mut transaction = connection.begin().await?;
    authority(&mut transaction).await?;
    let inserted = sqlx::query("INSERT INTO darkhorse_migration_v1.completions(operation_id) SELECT i.operation_id FROM darkhorse_migration_v1.intents i WHERE i.operation_id=$1 AND i.database_role=session_user AND i.manifest_count=(SELECT count(*) FROM darkhorse_migration_v1.targets t WHERE t.operation_id=i.operation_id) AND NOT EXISTS (SELECT 1 FROM darkhorse_migration_v1.targets t LEFT JOIN public._sqlx_migrations m ON m.version=t.version LEFT JOIN darkhorse_migration_v1.steps s ON s.operation_id=t.operation_id AND s.version=t.version WHERE t.operation_id=i.operation_id AND (m.version IS NULL OR NOT m.success OR m.checksum<>t.checksum OR (NOT t.already_applied AND s.version IS NULL)))")
        .bind(identifier(id)).execute(&mut *transaction).await?.rows_affected();
    if inserted != 1 {
        return Err(Error::Incompatible);
    }
    transaction.commit().await.map_err(|_| Error::Uncertain)
}
