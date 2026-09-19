use super::PostgresStore;
use darkhorse_application::sessions::{Ended, SessionManagement};
use darkhorse_domain::{
    authentication::SessionFacts,
    identity::{PrincipalId, SessionId},
    sessions::{self as policy, Cursor, Error, Page, Record},
};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;
mod records;
type Tx<'a> = Transaction<'a, Postgres>;
struct Actor {
    id: SessionId,
    principal: PrincipalId,
}
impl SessionManagement for PostgresStore {
    async fn sessions(&self, digest: [u8; 32], after: Option<Cursor>) -> Result<Page, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, false).await?;
        let actor = records::actor(&mut tx, digest).await?;
        let rows = records::list(&mut tx, actor.principal, after).await?;
        let now = now(&mut tx).await?;
        records::actor(&mut tx, digest).await?;
        let page = records::page(&actor, &rows, now)?;
        tx.commit().await.map_err(storage)?;
        Ok(page)
    }
    async fn end_session(&self, digest: [u8; 32], target: SessionId) -> Result<Ended, Error> {
        if let Some(ended) = preflight(self, digest, target).await? {
            return Ok(ended);
        }
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, true).await?;
        let actor = records::actor(&mut tx, digest).await?;
        let record = records::target(&mut tx, target, true).await?;
        policy::owns(actor.principal, records::principal(&record)?)?;
        records::actor(&mut tx, digest).await?;
        let now = now(&mut tx).await?;
        records::end(&mut tx, &record, actor.id, "session_ended", now).await?;
        tx.commit().await.map_err(storage)?;
        Ok(ended(&actor, target))
    }
}
async fn preflight(
    store: &PostgresStore,
    digest: [u8; 32],
    target: SessionId,
) -> Result<Option<Ended>, Error> {
    let mut tx = store.pool.begin().await.map_err(storage)?;
    lock(&mut tx, false).await?;
    let actor = records::actor(&mut tx, digest).await?;
    let row = records::target(&mut tx, target, false).await?;
    policy::owns(actor.principal, records::principal(&row)?)?;
    records::actor(&mut tx, digest).await?;
    let result = already_ended(&row, &actor, target)?;
    tx.commit().await.map_err(storage)?;
    Ok(result)
}
fn already_ended(row: &PgRow, actor: &Actor, target: SessionId) -> Result<Option<Ended>, Error> {
    Ok(row
        .try_get::<bool, _>("revoked")
        .map_err(storage)?
        .then(|| ended(actor, target)))
}
fn ended(actor: &Actor, target: SessionId) -> Ended {
    Ended {
        current: actor.id == target,
    }
}
pub(super) async fn lock(tx: &mut Tx<'_>, mutation: bool) -> Result<(), Error> {
    let query = if mutation {
        "SELECT singleton FROM security_state WHERE singleton AND NOT pg_is_in_recovery() FOR UPDATE"
    } else {
        "SELECT singleton FROM security_state WHERE singleton AND NOT pg_is_in_recovery() FOR SHARE"
    };
    sqlx::query(query)
        .fetch_one(&mut **tx)
        .await
        .map_err(storage)?;
    Ok(())
}
pub(super) async fn now(tx: &mut Tx<'_>) -> Result<u64, Error> {
    sqlx::query_scalar::<_, i64>("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint")
        .fetch_one(&mut **tx)
        .await
        .map_err(storage)?
        .try_into()
        .map_err(storage)
}
pub(super) async fn created(tx: &mut Tx<'_>, digest: [u8; 32]) -> Result<(), Error> {
    let row = sqlx::query("SELECT * FROM browser_sessions WHERE digest=$1")
        .bind(digest.as_slice())
        .fetch_one(&mut **tx)
        .await
        .map_err(storage)?;
    records::audit(tx, &row, None, "created", number(&row, "created_ms")?).await
}
pub(super) async fn end_by_handle(
    tx: &mut Tx<'_>,
    digest: [u8; 32],
    replacement: bool,
) -> Result<(), Error> {
    let row = sqlx::query("SELECT * FROM browser_sessions WHERE digest=$1 FOR UPDATE")
        .bind(digest.as_slice())
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?;
    let Some(row) = row else {
        return Ok(());
    };
    check_replacement(&row, replacement)?;
    let now = now(tx).await?;
    records::end(tx, &row, records::id(&row)?, event(replacement), now).await
}
fn event(replacement: bool) -> &'static str {
    if replacement {
        "replaced"
    } else {
        "signed_out"
    }
}
fn check_replacement(row: &PgRow, replacement: bool) -> Result<(), Error> {
    if replacement && row.try_get::<bool, _>("revoked").map_err(storage)? {
        return Err(Error::Unauthorized);
    }
    Ok(())
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

pub(super) async fn owner(
    tx: &mut Tx<'_>,
    digest: [u8; 32],
) -> Result<(PrincipalId, SessionId), Error> {
    let actor = records::actor(tx, digest).await?;
    Ok((actor.principal, actor.id))
}
