use super::{PostgresStore, registration::authority, sessions};
use darkhorse_application::{credentials::PreparedCredential, invitations::*};
use darkhorse_domain::{
    directory::Profile,
    identity::{InvitationId, PrincipalId},
    invitations::{self as policy, Error},
};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;
mod queue;
mod reads;
mod writes;
type Tx<'a> = Transaction<'a, Postgres>;
impl InvitationStore for PostgresStore {
    async fn invitation_preflight(&self, actor: [u8; 32]) -> Result<(), Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, false).await?;
        admin(&mut tx, actor, true).await?;
        tx.commit().await.map_err(storage)
    }
    async fn invite(
        &self,
        actor: [u8; 32],
        email: &str,
        material: Material,
        locale: Option<darkhorse_domain::localization::Locale>,
    ) -> Result<InvitationId, Error> {
        let email = policy::email(email)?;
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, true).await?;
        let (issuer, now) = admin(&mut tx, actor, true).await?;
        writes::invite(
            &mut tx,
            issuer,
            actor,
            &email,
            &material,
            now,
            locale.unwrap_or(self.default_locale),
        )
        .await?;
        admin(&mut tx, actor, true).await?;
        tx.commit().await.map_err(storage)?;
        Ok(material.id)
    }
    async fn invitations(&self, actor: [u8; 32]) -> Result<Vec<Record>, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, false).await?;
        let (_, now) = admin(&mut tx, actor, false).await?;
        let records = reads::list(&mut tx, now).await?;
        tx.commit().await.map_err(storage)?;
        Ok(records)
    }
    async fn revoke_invitation(&self, actor: [u8; 32], id: InvitationId) -> Result<(), Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, true).await?;
        let (issuer, now) = admin(&mut tx, actor, true).await?;
        writes::revoke(&mut tx, id, issuer, now).await?;
        admin(&mut tx, actor, true).await?;
        tx.commit().await.map_err(storage)
    }
    async fn admit_invitation(
        &self,
        digest: [u8; 32],
        email_key: &str,
    ) -> Result<InvitationId, Error> {
        // Cheap shared preflight keeps invalid public proofs off the exclusive write fence.
        reads::preflight(self, digest, email_key).await?;
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, true).await?;
        let row = reads::proof(&mut tx, digest, email_key).await?;
        let now = now(&mut tx).await?;
        reads::check(&row, now)?;
        writes::admit(&mut tx, &row, now).await?;
        tx.commit().await.map_err(storage)?;
        id(&row)
    }
    async fn accept_invitation(
        &self,
        expected: InvitationId,
        digest: [u8; 32],
        profile: Profile,
        credential: PreparedCredential,
    ) -> Result<PrincipalId, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, true).await?;
        let row = reads::proof(&mut tx, digest, profile.email_key()).await?;
        reads::acceptance(&row, expected, now(&mut tx).await?)?;
        let principal = credential.principal_id;
        writes::create(&mut tx, &profile, &credential).await?;
        // Recheck expiry after writes. Issuer rows remain locked and validated;
        // the newly inserted recipient must not be mistaken for a pre-existing account.
        let now = now(&mut tx).await?;
        reads::check(&row, now)?;
        writes::consume(&mut tx, expected, principal, now).await?;
        tx.commit().await.map_err(storage)?;
        Ok(principal)
    }
}
async fn lock(tx: &mut Tx<'_>, mutation: bool) -> Result<(), Error> {
    sessions::lock(tx, mutation).await.map_err(storage)
}
async fn now(tx: &mut Tx<'_>) -> Result<u64, Error> {
    sessions::now(tx).await.map_err(storage)
}
async fn admin(
    tx: &mut Tx<'_>,
    actor: [u8; 32],
    mutation: bool,
) -> Result<(PrincipalId, u64), Error> {
    authority::actor(tx, actor, mutation)
        .await
        .map_err(admin_error)
}
fn admin_error(e: darkhorse_domain::registration::RegistrationError) -> Error {
    use darkhorse_domain::registration::RegistrationError as R;
    match e {
        R::Unauthorized => Error::Unauthorized,
        R::Forbidden => Error::Forbidden,
        R::RecentAuthenticationRequired => Error::RecentAuthentication,
        _ => Error::Unavailable,
    }
}
async fn audit(
    tx: &mut Tx<'_>,
    id: InvitationId,
    actor: Option<PrincipalId>,
    event: &str,
    attempt: u16,
    now: u64,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO invitation_audit(invitation_id,actor_id,event,attempt,occurred_ms) VALUES($1,$2,$3,$4,$5)")
 .bind(uuid(id.as_u128()))
        .bind(actor.map(|id|uuid(id.as_u128())))
        .bind(event)
        .bind(attempt as i16)
        .bind(now as i64)
        .execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
fn id(row: &PgRow) -> Result<InvitationId, Error> {
    InvitationId::from_u128(row.try_get::<Uuid, _>("id").map_err(storage)?.as_u128())
        .map_err(storage)
}
fn number(row: &PgRow, key: &str) -> Result<u64, Error> {
    row.try_get::<i64, _>(key)
        .map_err(storage)?
        .try_into()
        .map_err(storage)
}
fn uuid(n: u128) -> Uuid {
    Uuid::from_u128(n)
}
fn storage<T>(_: T) -> Error {
    Error::Unavailable
}
