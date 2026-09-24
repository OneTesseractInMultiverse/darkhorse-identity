use super::{PostgresStore, registration::authority, sessions};
use darkhorse_application::admin_catalog::*;
use darkhorse_domain::{
    admin_catalog::{Change, Query},
    identity::*,
    registration::{RegistrationError as Error, next_revision},
};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use std::num::NonZeroU128;
use uuid::Uuid;
mod reads;
mod writes;
type Tx<'a> = Transaction<'a, Postgres>;
pub(super) async fn list_current(
    tx: &mut Tx<'_>,
    target: List,
    query: &Query,
) -> Result<Page, Error> {
    reads::list(tx, target, query).await
}
impl CatalogStore for PostgresStore {
    async fn list(&self, actor: [u8; 32], target: List, query: Query) -> Result<Page, Error> {
        query.validate()?;
        let mut tx = self.pool.begin().await.map_err(storage)?;
        fence(&mut tx, false).await?;
        authority::actor(&mut tx, actor, false).await?;
        let page = list_current(&mut tx, target, &query).await?;
        authority::actor(&mut tx, actor, false).await?;
        tx.commit().await.map_err(storage)?;
        Ok(page)
    }
    async fn view(&self, actor: [u8; 32], target: Target) -> Result<View, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        fence(&mut tx, false).await?;
        authority::actor(&mut tx, actor, false).await?;
        let view = reads::view(&mut tx, target).await?;
        authority::actor(&mut tx, actor, false).await?;
        tx.commit().await.map_err(storage)?;
        Ok(view)
    }
    async fn preflight(
        &self,
        actor: [u8; 32],
        revision: u64,
        _change: &Change,
    ) -> Result<(), Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        fence(&mut tx, false).await?;
        authority::actor(&mut tx, actor, true).await?;
        next_revision(reads::revision(&mut tx).await?, revision)?;
        tx.rollback().await.map_err(storage)
    }
    async fn execute(
        &self,
        actor: [u8; 32],
        revision: u64,
        change: Change,
        identifier: Option<NonZeroU128>,
    ) -> Result<Written, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        fence(&mut tx, true).await?;
        authority::actor(&mut tx, actor, true).await?;
        next_revision(reads::revision(&mut tx).await?, revision)?;
        let (principal, session) = sessions::owner(&mut tx, actor).await.map_err(storage)?;
        let plan = writes::prepare(&mut tx, &change, identifier).await?;
        let (_, now) = authority::actor(&mut tx, actor, true).await?;
        let written = writes::apply(&mut tx, plan, &change, principal, session, now).await?;
        tx.commit().await.map_err(constraint)?;
        Ok(written)
    }
}
async fn fence(tx: &mut Tx<'_>, write: bool) -> Result<(), Error> {
    sessions::lock(tx, write).await.map_err(storage)
}
fn uuid(value: u128) -> Uuid {
    Uuid::from_u128(value)
}
fn identifier(row: &PgRow, key: &str) -> Result<u128, Error> {
    Ok(row.try_get::<Uuid, _>(key).map_err(storage)?.as_u128())
}
fn number(row: &PgRow, key: &str) -> Result<u64, Error> {
    row.try_get::<i64, _>(key)
        .map_err(storage)?
        .try_into()
        .map_err(storage)
}
fn storage<T>(_: T) -> Error {
    Error::Unavailable
}
fn constraint(e: sqlx::Error) -> Error {
    match e.as_database_error().and_then(|e| e.code()).as_deref() {
        Some("23505") => Error::Conflict,
        Some("23503" | "23514") => Error::Invalid,
        _ => Error::Unavailable,
    }
}
