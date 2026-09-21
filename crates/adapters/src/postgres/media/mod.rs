use super::{PostgresStore, profiles, registration::authority, sessions};
use darkhorse_application::media::*;
use darkhorse_domain::{
    identity::{AssetId, PrincipalId},
    media::{self as policy, Kind},
    profiles::{Error, revision},
};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;
mod records;
type Tx<'a> = Transaction<'a, Postgres>;
impl Store for PostgresStore {
    async fn reserve(
        &self,
        digest: [u8; 32],
        target: Target,
        expected: u64,
    ) -> Result<Ticket, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        profiles::lock(&mut tx, true).await?;
        let (_, target) = authorize(&mut tx, digest, target, true).await?;
        let (current, _) = records::current(&mut tx, target).await?;
        let (actor, _) = authorize(&mut tx, digest, target, true).await?;
        revision(current, expected)?;
        records::budget(&mut tx, actor.principal, actor.now).await?;
        let id = AssetId::from_u128(Uuid::new_v4().as_u128()).map_err(storage)?;
        records::reserve(&mut tx, id, target, &actor).await?;
        tx.commit().await.map_err(storage)?;
        Ok(Ticket { id, target })
    }
    async fn attach(
        &self,
        digest: [u8; 32],
        ticket: Ticket,
        expected: u64,
        asset: &Prepared,
    ) -> Result<u64, Error> {
        policy::body_size(asset.bytes.len())?;
        policy::dimensions(asset.width, asset.height)?;
        let mut tx = self.pool.begin().await.map_err(storage)?;
        profiles::lock(&mut tx, true).await?;
        let (_, target) = authorize(&mut tx, digest, ticket.target, true).await?;
        let (current, old) = records::current(&mut tx, target).await?;
        let (actor, _) = authorize(&mut tx, digest, target, true).await?;
        let next = revision(current, expected)?;
        records::validate_ticket(&mut tx, &ticket, &actor).await?;
        records::publish(&mut tx, ticket.id, asset).await?;
        records::set(&mut tx, target, Some(ticket.id), next).await?;
        records::retire(&mut tx, old, actor.now).await?;
        records::audit(&mut tx, &actor, target, Some(ticket.id), next).await?;
        tx.commit().await.map_err(storage)?;
        Ok(next)
    }
    async fn remove(&self, digest: [u8; 32], target: Target, expected: u64) -> Result<u64, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        profiles::lock(&mut tx, true).await?;
        let (_, target) = authorize(&mut tx, digest, target, true).await?;
        let (current, old) = records::current(&mut tx, target).await?;
        let (actor, _) = authorize(&mut tx, digest, target, true).await?;
        let next = revision(current, expected)?;
        if old.is_none() {
            tx.commit().await.map_err(storage)?;
            return Ok(current);
        }
        records::set(&mut tx, target, None, next).await?;
        records::retire(&mut tx, old, actor.now).await?;
        records::audit(&mut tx, &actor, target, None, next).await?;
        tx.commit().await.map_err(storage)?;
        Ok(next)
    }
    async fn asset(
        &self,
        digest: Option<[u8; 32]>,
        target: Target,
    ) -> Result<Option<Asset>, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        profiles::lock(&mut tx, false).await?;
        let target = match target.kind {
            Kind::Portrait => {
                authorize(&mut tx, digest.ok_or(Error::Unauthorized)?, target, false)
                    .await?
                    .1
            }
            _ => public_target(target)?,
        };
        let (_, id) = records::current(&mut tx, target).await?;
        let asset = records::asset(&mut tx, id, target).await?;
        if target.kind == Kind::Portrait {
            authorize(&mut tx, digest.ok_or(Error::Unauthorized)?, target, false).await?;
        }
        tx.commit().await.map_err(storage)?;
        Ok(asset)
    }
    async fn branding(&self, digest: [u8; 32]) -> Result<Branding, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        profiles::lock(&mut tx, false).await?;
        authorize(
            &mut tx,
            digest,
            Target {
                kind: Kind::Logo,
                principal: None,
            },
            false,
        )
        .await?;
        let (revision, logo) = records::current(
            &mut tx,
            Target {
                kind: Kind::Logo,
                principal: None,
            },
        )
        .await?;
        let (_, background) = records::current(
            &mut tx,
            Target {
                kind: Kind::Background,
                principal: None,
            },
        )
        .await?;
        authorize(
            &mut tx,
            digest,
            Target {
                kind: Kind::Logo,
                principal: None,
            },
            false,
        )
        .await?;
        tx.commit().await.map_err(storage)?;
        Ok(Branding {
            revision,
            logo: logo.is_some(),
            background: background.is_some(),
        })
    }
    async fn garbage(&self) -> Result<Vec<AssetId>, Error> {
        records::garbage(self).await
    }
    async fn cleaned(&self, id: AssetId) -> Result<(), Error> {
        records::cleaned(self, id).await
    }
}
async fn authorize(
    tx: &mut Tx<'_>,
    digest: [u8; 32],
    target: Target,
    fresh: bool,
) -> Result<(profiles::Actor, Target), Error> {
    if target.kind != Kind::Portrait {
        public_target(target)?;
        authority::actor(tx, digest, fresh)
            .await
            .map_err(|e| match e {
                darkhorse_domain::registration::RegistrationError::Forbidden => Error::Forbidden,
                darkhorse_domain::registration::RegistrationError::Unauthorized => {
                    Error::Unauthorized
                }
                darkhorse_domain::registration::RegistrationError::RecentAuthenticationRequired => {
                    Error::RecentAuthentication
                }
                _ => Error::Unavailable,
            })?;
    }
    let actor = profiles::actor(tx, digest, target.principal, fresh).await?;
    let principal = if target.kind == Kind::Portrait {
        Some(target.principal.unwrap_or(actor.principal))
    } else {
        None
    };
    Ok((
        actor,
        Target {
            principal,
            ..target
        },
    ))
}
fn public_target(target: Target) -> Result<Target, Error> {
    if target.kind == Kind::Portrait || target.principal.is_some() {
        Err(Error::Invalid)
    } else {
        Ok(target)
    }
}
fn kind(kind: Kind) -> &'static str {
    match kind {
        Kind::Portrait => "portrait",
        Kind::Logo => "logo",
        Kind::Background => "background",
    }
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
impl PostgresStore {
    pub async fn bind_object_storage(&self, fingerprint: [u8; 32]) -> Result<(), Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        profiles::lock(&mut tx, true).await?;
        sqlx::query("INSERT INTO media_configuration(singleton,storage_fingerprint) VALUES(true,$1) ON CONFLICT DO NOTHING").bind(fingerprint.as_slice()).execute(&mut *tx).await.map_err(storage)?;
        let stored: Vec<u8> = sqlx::query_scalar(
            "SELECT storage_fingerprint FROM media_configuration WHERE singleton",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(storage)?;
        if stored != fingerprint {
            return Err(Error::Conflict);
        }
        tx.commit().await.map_err(storage)?;
        Ok(())
    }
}
