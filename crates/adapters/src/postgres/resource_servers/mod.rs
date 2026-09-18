use super::{PostgresStore, registration::authority};
use darkhorse_application::resource_servers::*;
use darkhorse_domain::{
    identity::*,
    registration::{RegistrationError as Error, next_revision, overlap_deadline},
};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;
mod writes;
type Tx<'a> = Transaction<'a, Postgres>;
impl Store for PostgresStore {
    async fn preflight(&self, actor: [u8; 32], command: Command) -> Result<(), Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        authority::lock(&mut tx).await?;
        let (_, now) = authority::actor(&mut tx, actor, true).await?;
        check(&mut tx, command, now).await?;
        tx.rollback().await.map_err(storage)
    }
    async fn execute(
        &self,
        actor: [u8; 32],
        command: Command,
        secret: Option<Verifier>,
    ) -> Result<Record, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        authority::lock(&mut tx).await?;
        let (principal, now) = authority::actor(&mut tx, actor, true).await?;
        check(&mut tx, command, now).await?;
        writes::apply(&mut tx, command, secret, now).await?;
        let record = read(&mut tx, command.target).await?;
        writes::audit(&mut tx, principal, command, &record, now).await?;
        tx.commit().await.map_err(storage)?;
        Ok(record)
    }
    async fn read(&self, actor: [u8; 32], target: Target) -> Result<Record, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        super::oidc::authority::lock(&mut tx)
            .await
            .map_err(storage)?;
        authority::actor(&mut tx, actor, false).await?;
        let record = read(&mut tx, target).await?;
        tx.commit().await.map_err(storage)?;
        Ok(record)
    }
}
async fn check(tx: &mut Tx<'_>, command: Command, now: u64) -> Result<(), Error> {
    sqlx::query("SELECT id FROM protected_resources WHERE application_id=$1 AND id=$2")
        .bind(uuid(command.target.application.as_u128()))
        .bind(uuid(command.target.resource.as_u128()))
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?
        .ok_or(Error::NotFound)?;
    let revision: Option<i64> =
        sqlx::query_scalar("SELECT revision FROM resource_introspection WHERE resource_id=$1")
            .bind(uuid(command.target.resource.as_u128()))
            .fetch_optional(&mut **tx)
            .await
            .map_err(storage)?;
    validate(command.change, revision, now)
}
fn validate(change: Change, current: Option<i64>, now: u64) -> Result<(), Error> {
    match change {
        Change::Register if current.is_some() => Err(Error::Conflict),
        Change::Register => Ok(()),
        Change::Rotate {
            revision,
            overlap_seconds,
        } => {
            next_revision(
                current
                    .ok_or(Error::NotFound)?
                    .try_into()
                    .map_err(storage)?,
                revision,
            )?;
            overlap_deadline(now, overlap_seconds)?;
            Ok(())
        }
        Change::SetActive { revision, .. } => {
            next_revision(
                current
                    .ok_or(Error::NotFound)?
                    .try_into()
                    .map_err(storage)?,
                revision,
            )?;
            Ok(())
        }
    }
}
async fn read(tx: &mut Tx<'_>, target: Target) -> Result<Record, Error> {
    let (active,revision):(bool,i64)=sqlx::query_as("SELECT active,revision FROM resource_introspection WHERE application_id=$1 AND resource_id=$2")
        .bind(uuid(target.application.as_u128())).bind(uuid(target.resource.as_u128())).fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(Error::NotFound)?;
    let rows=sqlx::query("SELECT id,created_ms,expires_ms FROM resource_introspection_secrets WHERE resource_id=$1 AND NOT retired ORDER BY created_ms,id LIMIT 2")
        .bind(uuid(target.resource.as_u128())).fetch_all(&mut **tx).await.map_err(storage)?;
    Ok(Record {
        target,
        active,
        revision: revision.try_into().map_err(storage)?,
        secrets: rows.iter().map(metadata).collect::<Result<_, _>>()?,
    })
}
fn metadata(row: &PgRow) -> Result<SecretMetadata, Error> {
    Ok(SecretMetadata {
        id: CredentialId::from_u128(row.try_get::<Uuid, _>("id").map_err(storage)?.as_u128())
            .map_err(storage)?,
        created_ms: row
            .try_get::<i64, _>("created_ms")
            .map_err(storage)?
            .try_into()
            .map_err(storage)?,
        expires_ms: row
            .try_get::<Option<i64>, _>("expires_ms")
            .map_err(storage)?
            .map(|n| u64::try_from(n).map_err(storage))
            .transpose()?,
    })
}
fn uuid(id: u128) -> Uuid {
    Uuid::from_u128(id)
}
fn storage<T>(_: T) -> Error {
    Error::Unavailable
}
