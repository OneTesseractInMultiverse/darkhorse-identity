mod inspect;
mod journal;
mod plan;
use super::{MIGRATOR, PostgresStore};
use darkhorse_domain::identity::OperationId;
pub use inspect::{Inspection, Step};
use sqlx::{
    Connection, PgConnection,
    migrate::{Migrate, Migration},
};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Incompatible,
    Unavailable,
    Uncertain,
}
impl From<sqlx::Error> for Error {
    fn from(_: sqlx::Error) -> Self {
        Self::Unavailable
    }
}
impl From<sqlx::migrate::MigrateError> for Error {
    fn from(_: sqlx::migrate::MigrateError) -> Self {
        Self::Unavailable
    }
}

impl PostgresStore {
    pub async fn migrate_operation(&self, id: OperationId) -> Result<(), Error> {
        let mut connection = self.pool.acquire().await?.detach();
        let result = run(&mut connection, id, MIGRATOR.iter().as_slice()).await;
        // Never return a session holding SQLx's advisory lock to the pool.
        // Dropping the detached connection on cancellation closes its socket too.
        let _ = connection.close().await;
        result
    }
    pub async fn inspect_migration(&self, id: OperationId) -> Result<Option<Inspection>, Error> {
        inspect::inspect(&self.pool, id).await
    }
}

async fn run(
    connection: &mut PgConnection,
    id: OperationId,
    manifest: &[Migration],
) -> Result<(), Error> {
    authority(connection).await?;
    connection.lock().await?;
    let baseline = journal::prepare(connection, id, manifest).await?;
    apply_pending(connection, id, &manifest[baseline..])
        .await
        .map_err(|_| Error::Uncertain)?;
    journal::complete(connection, id)
        .await
        .map_err(|_| Error::Uncertain)
}
async fn authority(connection: &mut PgConnection) -> Result<(), Error> {
    let authorized: bool = sqlx::query_scalar("SELECT NOT pg_is_in_recovery() AND session_user=pg_get_userbyid(datdba) FROM pg_database WHERE datname=current_database()")
        .fetch_one(connection).await?;
    if authorized {
        Ok(())
    } else {
        Err(Error::Unavailable)
    }
}
async fn apply_pending(
    connection: &mut PgConnection,
    id: OperationId,
    pending: &[Migration],
) -> Result<(), Error> {
    for migration in pending {
        apply_one(connection, id, migration).await?;
    }
    Ok(())
}
async fn apply_one(
    connection: &mut PgConnection,
    id: OperationId,
    migration: &Migration,
) -> Result<(), Error> {
    let mut transaction = connection.begin().await?;
    // SQLx 0.9 uses a savepoint inside this outer transaction. Its history and
    // execution-time update are committed together with our step receipt.
    transaction
        .apply("public._sqlx_migrations", migration)
        .await?;
    sqlx::query("INSERT INTO darkhorse_migration_v1.steps(operation_id,version) VALUES ($1,$2)")
        .bind(identifier(id))
        .bind(migration.version)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await.map_err(|_| Error::Uncertain)
}
fn identifier(id: OperationId) -> Uuid {
    Uuid::from_u128(id.as_u128())
}
