use super::{PostgresStore, sessions};
use darkhorse_application::email_verification::{Material, Status, VerificationStore};
use darkhorse_domain::{
    email_verification::{self as policy, Error, Proof},
    identity::{EmailVerificationId, PrincipalId, SessionId},
};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;
mod queue;
type Tx<'a> = Transaction<'a, Postgres>;
struct Actor {
    principal: PrincipalId,
    session: SessionId,
    email: String,
    epoch: u64,
    verified: bool,
}
impl VerificationStore for PostgresStore {
    async fn email_status(&self, digest: [u8; 32]) -> Result<Status, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, false).await?;
        let actor = actor(&mut tx, digest).await?;
        tx.commit().await.map_err(storage)?;
        Ok(Status {
            email: actor.email,
            verified: actor.verified,
        })
    }
    async fn request_verification(
        &self,
        digest: [u8; 32],
        material: Material,
    ) -> Result<(), Error> {
        let status = self.email_status(digest).await?;
        if status.verified {
            return Ok(());
        }
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, true).await?;
        let actor = actor(&mut tx, digest).await?;
        if !actor.verified {
            enqueue(&mut tx, &actor, &material).await?;
        }
        // Authentication can expire while waiting for work; rollback on failure.
        self::actor(&mut tx, digest).await?;
        tx.commit().await.map_err(storage)
    }
    async fn verify_email(&self, digest: [u8; 32], proof: [u8; 32]) -> Result<(), Error> {
        preflight(self, digest, proof).await?;
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, true).await?;
        let actor = actor(&mut tx, digest).await?;
        let row = proof_row(&mut tx, actor.principal, proof).await?;
        let now = now(&mut tx).await?;
        check(&row, &actor, now)?;
        confirm(&mut tx, &actor, &row, now).await?;
        self::actor(&mut tx, digest).await?;
        tx.commit().await.map_err(storage)
    }
}
async fn preflight(store: &PostgresStore, digest: [u8; 32], proof: [u8; 32]) -> Result<(), Error> {
    let mut tx = store.pool.begin().await.map_err(storage)?;
    lock(&mut tx, false).await?;
    let actor = actor(&mut tx, digest).await?;
    let row = proof_row(&mut tx, actor.principal, proof).await?;
    check(&row, &actor, now(&mut tx).await?)?;
    tx.commit().await.map_err(storage)
}
async fn actor(tx: &mut Tx<'_>, digest: [u8; 32]) -> Result<Actor, Error> {
    let owner = sessions::owner(tx, digest).await.map_err(auth_error)?;
    let row=sqlx::query("SELECT email,credential_epoch,email_verified_ms IS NOT NULL AS verified FROM principals WHERE id=$1")
        .bind(Uuid::from_u128(owner.0.as_u128())).fetch_one(&mut **tx).await.map_err(storage)?;
    Ok(Actor {
        principal: owner.0,
        session: owner.1,
        email: row.try_get("email").map_err(storage)?,
        epoch: number(&row, "credential_epoch")?,
        verified: row.try_get("verified").map_err(storage)?,
    })
}
fn auth_error(error: darkhorse_domain::sessions::Error) -> Error {
    match error {
        darkhorse_domain::sessions::Error::Unauthorized => Error::Unauthorized,
        _ => Error::Unavailable,
    }
}
async fn lock(tx: &mut Tx<'_>, mutation: bool) -> Result<(), Error> {
    sessions::lock(tx, mutation).await.map_err(storage)
}
async fn now(tx: &mut Tx<'_>) -> Result<u64, Error> {
    sessions::now(tx).await.map_err(storage)
}
async fn proof_row(tx: &mut Tx<'_>, owner: PrincipalId, digest: [u8; 32]) -> Result<PgRow, Error> {
    sqlx::query("SELECT * FROM email_verifications WHERE principal_id=$1 AND digest=$2 FOR SHARE")
        .bind(Uuid::from_u128(owner.as_u128()))
        .bind(digest.as_slice())
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?
        .ok_or(Error::Invalid)
}
fn check(row: &PgRow, actor: &Actor, now: u64) -> Result<(), Error> {
    policy::redeem(
        &Proof {
            email: row.try_get("email").map_err(storage)?,
            epoch: number(row, "credential_epoch")?,
            created_ms: number(row, "created_ms")?,
            expires_ms: number(row, "expires_ms")?,
            consumed: row.try_get("consumed").map_err(storage)?,
        },
        &actor.email,
        actor.epoch,
        now,
    )
}
async fn enqueue(tx: &mut Tx<'_>, actor: &Actor, material: &Material) -> Result<(), Error> {
    let now = now(tx).await?;
    let counts=sqlx::query("SELECT (SELECT max(occurred_ms) FROM email_verification_audit WHERE principal_id=$1 AND event='requested') AS last_ms,(SELECT count(*) FROM email_verification_audit WHERE principal_id=$1 AND event='requested' AND occurred_ms>$2) AS daily,((SELECT count(*) FROM email_verifications WHERE delivery_state='queued' AND expires_ms>$3)+(SELECT count(*) FROM invitations WHERE delivery_state='queued' AND expires_ms>$3)) AS queued")
        .bind(Uuid::from_u128(actor.principal.as_u128())).bind(now.saturating_sub(policy::DAY_MS) as i64).bind(now as i64).fetch_one(&mut **tx).await.map_err(storage)?;
    let expires = request_expiry(&counts, now)?;
    sqlx::query("INSERT INTO email_verifications(principal_id,id,actor_session_id,email,credential_epoch,digest,seed,created_ms,expires_ms,next_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$8) ON CONFLICT(principal_id) DO UPDATE SET id=excluded.id,actor_session_id=excluded.actor_session_id,email=excluded.email,credential_epoch=excluded.credential_epoch,digest=excluded.digest,seed=excluded.seed,created_ms=excluded.created_ms,expires_ms=excluded.expires_ms,next_ms=excluded.next_ms,consumed=false,attempts=0,delivery_state='queued'")
        .bind(Uuid::from_u128(actor.principal.as_u128())).bind(Uuid::from_u128(material.id.as_u128())).bind(Uuid::from_u128(actor.session.as_u128())).bind(&actor.email).bind(actor.epoch as i64).bind(material.digest.as_slice()).bind(material.seed.as_slice()).bind(now as i64).bind(expires as i64).execute(&mut **tx).await.map_err(storage)?;
    audit(
        tx,
        actor.principal,
        material.id,
        Some(actor.session),
        "requested",
        0,
        now,
    )
    .await
}
fn request_expiry(row: &PgRow, now: u64) -> Result<u64, Error> {
    let last: Option<i64> = row.try_get("last_ms").map_err(storage)?;
    policy::request(
        last.map(|n| n as u64),
        number(row, "daily")?,
        number(row, "queued")?,
        now,
    )
}
async fn confirm(tx: &mut Tx<'_>, actor: &Actor, row: &PgRow, now: u64) -> Result<(), Error> {
    sqlx::query("UPDATE email_verifications SET consumed=true,seed=NULL,delivery_state=CASE WHEN delivery_state='queued' THEN 'cancelled' ELSE delivery_state END WHERE principal_id=$1")
        .bind(Uuid::from_u128(actor.principal.as_u128())).execute(&mut **tx).await.map_err(storage)?;
    sqlx::query("UPDATE principals SET email_verified_ms=$2,revision=revision+1 WHERE id=$1")
        .bind(Uuid::from_u128(actor.principal.as_u128()))
        .bind(now as i64)
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    audit(
        tx,
        actor.principal,
        id(row)?,
        Some(actor.session),
        "verified",
        0,
        now,
    )
    .await
}
async fn audit(
    tx: &mut Tx<'_>,
    owner: PrincipalId,
    id: EmailVerificationId,
    actor: Option<SessionId>,
    event: &str,
    attempt: u16,
    now: u64,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO email_verification_audit(principal_id,verification_id,actor_session_id,event,attempt,occurred_ms) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(Uuid::from_u128(owner.as_u128())).bind(Uuid::from_u128(id.as_u128())).bind(actor.map(|s|Uuid::from_u128(s.as_u128()))).bind(event).bind(attempt as i16).bind(now as i64).execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
fn id(row: &PgRow) -> Result<EmailVerificationId, Error> {
    EmailVerificationId::from_u128(row.try_get::<Uuid, _>("id").map_err(storage)?.as_u128())
        .map_err(storage)
}
fn principal(row: &PgRow) -> Result<PrincipalId, Error> {
    PrincipalId::from_u128(
        row.try_get::<Uuid, _>("principal_id")
            .map_err(storage)?
            .as_u128(),
    )
    .map_err(storage)
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
