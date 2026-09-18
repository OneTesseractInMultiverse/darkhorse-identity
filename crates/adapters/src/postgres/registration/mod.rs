use super::PostgresStore;
use darkhorse_application::registration::*;
use darkhorse_domain::{identity::*, registration::*};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;
pub(in crate::postgres) mod authority;
mod records;
mod writes;
type Tx<'a> = Transaction<'a, Postgres>;
type Error = RegistrationError;

impl RegistrationStore for PostgresStore {
    async fn preflight(&self, actor: [u8; 32], command: &Command) -> Result<(), Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        authority::lock(&mut tx).await?;
        let (_, now) = authority::actor(&mut tx, actor, true).await?;
        authority::command(&mut tx, command, now).await?;
        tx.rollback().await.map_err(storage)
    }
    async fn execute(
        &self,
        actor: [u8; 32],
        command: &Command,
        prepared: Prepared,
    ) -> Result<Record, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        authority::lock(&mut tx).await?;
        let (principal, now) = authority::actor(&mut tx, actor, true).await?;
        authority::command(&mut tx, command, now).await?;
        let record = writes::apply(&mut tx, command, prepared, now).await?;
        audit(&mut tx, principal, command, &record, now).await?;
        tx.commit().await.map_err(storage)?;
        Ok(record)
    }
    async fn read(&self, actor: [u8; 32], target: ReadTarget) -> Result<Record, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // Administrative reads share the authority fence; no previous decision is reused.
        sqlx::query("SELECT singleton FROM security_state WHERE singleton AND NOT pg_is_in_recovery() FOR SHARE")
            .fetch_one(&mut *tx).await.map_err(storage)?;
        authority::actor(&mut tx, actor, false).await?;
        let record = records::read(&mut tx, target).await?;
        tx.commit().await.map_err(storage)?;
        Ok(record)
    }
}
impl ClientAuthenticationStore for PostgresStore {
    async fn authenticate_client(
        &self,
        client: ClientId,
        verifier: [u8; 32],
    ) -> Result<ClientRecord, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        let row = sqlx::query("SELECT c.application_id,s.created_ms,s.expires_ms,s.retired,floor(extract(epoch FROM clock_timestamp())*1000)::bigint AS now_ms FROM oauth_clients c JOIN applications a ON a.id=c.application_id JOIN oauth_client_secrets s ON s.client_id=c.id WHERE c.id=$1 AND s.verifier=$2 AND c.active AND a.active AND NOT pg_is_in_recovery()")
            .bind(uuid(client.as_u128())).bind(verifier.as_slice()).fetch_optional(&mut *tx).await.map_err(storage)?.ok_or(Error::Unauthorized)?;
        if !secret_live(
            number(&row, "created_ms")?,
            optional_number(&row, "expires_ms")?,
            row.try_get("retired").map_err(storage)?,
            number(&row, "now_ms")?,
        ) {
            return Err(Error::Unauthorized);
        }
        let application =
            ApplicationId::from_u128(identifier(&row, "application_id")?).map_err(storage)?;
        let record = records::client(&mut tx, application, client).await?;
        tx.commit().await.map_err(storage)?;
        Ok(record)
    }
}
async fn audit(
    tx: &mut Tx<'_>,
    actor: PrincipalId,
    command: &Command,
    record: &Record,
    now: u64,
) -> Result<(), Error> {
    let (target, event) = audit_values(command, record);
    sqlx::query("INSERT INTO registration_audit (actor_id,target_id,event,occurred_ms) VALUES ($1,$2,$3,$4)")
        .bind(uuid(actor.as_u128())).bind(uuid(target)).bind(event).bind(integer(now)?).execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
fn audit_values(command: &Command, record: &Record) -> (u128, &'static str) {
    let target = match record {
        Record::Application(r) => r.id.as_u128(),
        Record::Resource(r) => r.id.as_u128(),
        Record::Scope(r) => r.id.as_u128(),
        Record::Client(r) => r.id.as_u128(),
    };
    let event = match command {
        Command::CreateApplication(_) => "application_created",
        Command::UpdateApplication { .. } => "application_updated",
        Command::CreateResource { .. } => "resource_created",
        Command::CreateScope { .. } => "scope_created",
        Command::CreateClient { .. } => "client_created",
        Command::UpdateClient { .. } => "client_updated",
        Command::RotateSecret { .. } => "secret_rotated",
        Command::RetireSecret { .. } => "secret_retired",
    };
    (target, event)
}
fn uuid(id: u128) -> Uuid {
    Uuid::from_u128(id)
}
fn integer(n: u64) -> Result<i64, Error> {
    n.try_into().map_err(storage)
}
fn number(row: &PgRow, key: &str) -> Result<u64, Error> {
    row.try_get::<i64, _>(key)
        .map_err(storage)?
        .try_into()
        .map_err(storage)
}
fn optional_number(row: &PgRow, key: &str) -> Result<Option<u64>, Error> {
    row.try_get::<Option<i64>, _>(key)
        .map_err(storage)?
        .map(|n| n.try_into().map_err(storage))
        .transpose()
}
fn identifier(row: &PgRow, key: &str) -> Result<u128, Error> {
    Ok(row.try_get::<Uuid, _>(key).map_err(storage)?.as_u128())
}
fn storage<T>(_: T) -> Error {
    Error::Unavailable
}
fn constraint(error: sqlx::Error) -> Error {
    match error.as_database_error().and_then(|e| e.code()).as_deref() {
        Some("23505") => Error::Conflict,
        Some("23503" | "23514") => Error::Invalid,
        _ => Error::Unavailable,
    }
}
