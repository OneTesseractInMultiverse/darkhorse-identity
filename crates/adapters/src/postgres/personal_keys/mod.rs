//! Transaction ownership for personal credentials and current-primary authority.
use super::{PostgresStore, sessions};
use darkhorse_application::personal_keys::*;
use darkhorse_domain::{
    AccountStatus,
    authorization::*,
    identity::*,
    personal_keys::{self as policy, Error, ExpiryPolicy, Request},
};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use std::collections::BTreeSet;
use uuid::Uuid;
mod authority;
mod introspection;
mod projection;
mod records;
type Tx<'a> = Transaction<'a, Postgres>;
struct Actor {
    principal: PrincipalId,
    session: SessionId,
    epoch: u64,
    now: u64,
}
impl Store for PostgresStore {
    async fn options(&self, digest: [u8; 32], after: Option<ResourceId>) -> Result<Options, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, false).await?;
        let actor = authority::actor(&mut tx, digest, false).await?;
        let (policy, revision) = authority::policy(&mut tx).await?;
        let (items, next) = records::options(&mut tx, actor.principal, after).await?;
        authority::actor(&mut tx, digest, false).await?;
        tx.commit().await.map_err(storage)?;
        Ok(Options {
            policy,
            revision,
            items,
            next,
        })
    }
    async fn list(&self, digest: [u8; 32], after: Option<CredentialId>) -> Result<Page, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, false).await?;
        let actor = authority::actor(&mut tx, digest, false).await?;
        let page = records::list(&mut tx, &actor, after).await?;
        authority::actor(&mut tx, digest, false).await?;
        tx.commit().await.map_err(storage)?;
        Ok(page)
    }
    async fn preflight(&self, digest: [u8; 32], request: &Request) -> Result<(), Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, false).await?;
        let actor = authority::actor(&mut tx, digest, true).await?;
        authority::prepare(&mut tx, &actor, request).await?;
        tx.commit().await.map_err(storage)
    }
    async fn issue(
        &self,
        digest: [u8; 32],
        request: &Request,
        verifier: Verifier,
    ) -> Result<Record, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, true).await?;
        let actor = authority::actor(&mut tx, digest, true).await?;
        let prepared = authority::prepare(&mut tx, &actor, request).await?;
        // Time-based assurance is rechecked after all policy reads and locks.
        let actor = authority::actor(&mut tx, digest, true).await?;
        let record = records::issue(&mut tx, &actor, request, prepared, verifier).await?;
        tx.commit().await.map_err(storage)?;
        Ok(record)
    }
    async fn revoke(&self, digest: [u8; 32], key: CredentialId) -> Result<(), Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, true).await?;
        let actor = authority::actor(&mut tx, digest, true).await?;
        records::revoke(&mut tx, &actor, key).await?;
        authority::actor(&mut tx, digest, true).await?;
        tx.commit().await.map_err(storage)
    }
}
async fn lock(tx: &mut Tx<'_>, mutation: bool) -> Result<(), Error> {
    sessions::lock(tx, mutation).await.map_err(session_error)
}
fn session_error(error: darkhorse_domain::sessions::Error) -> Error {
    match error {
        darkhorse_domain::sessions::Error::Unauthorized => Error::Unauthorized,
        _ => Error::Unavailable,
    }
}
fn storage<T>(_: T) -> Error {
    Error::Unavailable
}
fn uuid(n: u128) -> Uuid {
    Uuid::from_u128(n)
}
fn number(row: &PgRow, key: &str) -> Result<u64, Error> {
    row.try_get::<i64, _>(key)
        .map_err(storage)?
        .try_into()
        .map_err(storage)
}
fn identifier(row: &PgRow, key: &str) -> Result<u128, Error> {
    Ok(row.try_get::<Uuid, _>(key).map_err(storage)?.as_u128())
}
fn optional_time(row: &PgRow, key: &str) -> Result<Option<u64>, Error> {
    row.try_get::<Option<i64>, _>(key)
        .map_err(storage)?
        .map(|n| n.try_into().map_err(storage))
        .transpose()
}
