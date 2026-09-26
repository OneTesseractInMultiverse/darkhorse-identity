use super::{PostgresStore, sessions};
use darkhorse_application::profiles::{Profile, Store};
use darkhorse_domain::{
    identity::{PrincipalId, SessionId},
    profiles::{self as policy, Error, Fields, Input, Phone},
};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;
mod language;
mod records;
type Tx<'a> = Transaction<'a, Postgres>;
pub(super) struct Actor {
    pub principal: PrincipalId,
    pub session: SessionId,
    pub now: u64,
}
impl Store for PostgresStore {
    async fn update_language(
        &self,
        digest: [u8; 32],
        expected: u64,
        locale: Option<darkhorse_domain::localization::Locale>,
    ) -> Result<Profile, Error> {
        language::update(self, digest, expected, locale).await
    }
    async fn profile(
        &self,
        digest: [u8; 32],
        target: Option<PrincipalId>,
    ) -> Result<Profile, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, false).await?;
        let actor = actor(&mut tx, digest, target, false).await?;
        let target = target.unwrap_or(actor.principal);
        let result = records::read(&mut tx, target).await?;
        self::actor(&mut tx, digest, Some(target), false).await?;
        tx.commit().await.map_err(storage)?;
        Ok(result)
    }
    async fn update_profile(
        &self,
        digest: [u8; 32],
        target: Option<PrincipalId>,
        expected: u64,
        fields: Fields,
    ) -> Result<Profile, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, true).await?;
        let actor = actor(&mut tx, digest, target, true).await?;
        let target = target.unwrap_or(actor.principal);
        // The global fence precedes every principal lock, as in directory administration.
        let current = records::read(&mut tx, target).await?;
        let next = policy::revision(current.revision, expected)?;
        let actor = self::actor(&mut tx, digest, Some(target), true).await?;
        if current.fields != fields {
            records::update(&mut tx, target, next, &fields).await?;
            records::audit(&mut tx, &actor, target, next).await?;
        }
        let result = records::read(&mut tx, target).await?;
        tx.commit().await.map_err(storage)?;
        Ok(result)
    }
}
pub(super) async fn lock(tx: &mut Tx<'_>, write: bool) -> Result<(), Error> {
    sessions::lock(tx, write).await.map_err(storage)
}
pub(super) async fn actor(
    tx: &mut Tx<'_>,
    digest: [u8; 32],
    target: Option<PrincipalId>,
    fresh: bool,
) -> Result<Actor, Error> {
    let (principal, session) = sessions::owner(tx, digest).await.map_err(|e| match e {
        darkhorse_domain::sessions::Error::Unavailable => Error::Unavailable,
        _ => Error::Unauthorized,
    })?;
    let row=sqlx::query("SELECT s.created_ms,EXISTS(SELECT 1 FROM platform_administrators a WHERE a.principal_id=s.principal_id) AS administrator FROM browser_sessions s WHERE s.public_id=$1").bind(uuid(session.as_u128())).fetch_one(&mut **tx).await.map_err(storage)?;
    policy::authorize(
        principal,
        target.unwrap_or(principal),
        row.try_get("administrator").map_err(storage)?,
    )?;
    let now = sessions::now(tx).await.map_err(storage)?;
    if fresh {
        policy::recent(number(&row, "created_ms")?, now)?;
    }
    Ok(Actor {
        principal,
        session,
        now,
    })
}
fn storage<T>(_: T) -> Error {
    Error::Unavailable
}
fn uuid(id: u128) -> Uuid {
    Uuid::from_u128(id)
}
fn number(row: &PgRow, key: &str) -> Result<u64, Error> {
    row.try_get::<i64, _>(key)
        .map_err(storage)?
        .try_into()
        .map_err(storage)
}
