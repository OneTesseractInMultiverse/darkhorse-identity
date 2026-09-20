use super::{PostgresStore, directory, registration::authority, sessions};
use darkhorse_application::admin_directory::*;
use darkhorse_domain::{
    AccountStatus,
    admin_directory::{Change, Error, Query, revision},
    directory::{AccountAction, DirectoryError},
    identity::*,
};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;
mod reads;
mod writes;
type Tx<'a> = Transaction<'a, Postgres>;
impl AdminDirectory for PostgresStore {
    async fn users(&self, actor: [u8; 32], query: Query) -> Result<Page, Error> {
        query.validate()?;
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, false).await?;
        let (actor_id, _) = authorize(&mut tx, actor, false).await?;
        let page = reads::list(&mut tx, actor_id, &query).await?;
        authorize(&mut tx, actor, false).await?;
        tx.commit().await.map_err(storage)?;
        Ok(page)
    }
    async fn user(&self, actor: [u8; 32], id: PrincipalId) -> Result<User, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, false).await?;
        authorize(&mut tx, actor, false).await?;
        let user = reads::user(&mut tx, id).await?;
        authorize(&mut tx, actor, false).await?;
        tx.commit().await.map_err(storage)?;
        Ok(user)
    }
    async fn access(
        &self,
        actor: [u8; 32],
        id: PrincipalId,
        application: Option<ApplicationId>,
    ) -> Result<Access, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, false).await?;
        authorize(&mut tx, actor, false).await?;
        let user = reads::user(&mut tx, id).await?;
        let applications = reads::applications(&mut tx).await?;
        let selected = reads::selection(&applications, application)?;
        let roles = match selected {
            Some(app) => reads::roles(&mut tx, id, uuid(app.as_u128())).await?,
            None => Vec::new(),
        };
        let policy_revision = reads::policy_revision(&mut tx).await?;
        authorize(&mut tx, actor, false).await?;
        tx.commit().await.map_err(storage)?;
        Ok(Access {
            user,
            policy_revision,
            applications,
            selected,
            roles,
        })
    }
    async fn update(
        &self,
        actor: [u8; 32],
        id: PrincipalId,
        expected: u64,
        change: Change,
    ) -> Result<User, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        lock(&mut tx, true).await?;
        authorize(&mut tx, actor, true).await?;
        let current = directory::locked_account(&mut tx, id)
            .await
            .map_err(directory_error)?;
        revision(current.revision, expected)?;
        let (principal, session) = sessions::owner(&mut tx, actor).await.map_err(|e| {
            if e == darkhorse_domain::sessions::Error::Unavailable {
                Error::Unavailable
            } else {
                Error::Unauthorized
            }
        })?;
        let (_, now) = authorize(&mut tx, actor, true).await?;
        if writes::apply(&mut tx, &current, &change).await? {
            writes::audit(
                &mut tx,
                writes::Audit {
                    actor: principal,
                    session,
                    target: id,
                    revision: expected + 1,
                    now,
                },
                &change,
            )
            .await?;
        }
        let user = reads::user(&mut tx, id).await?;
        tx.commit().await.map_err(storage)?;
        Ok(user)
    }
}
async fn lock(tx: &mut Tx<'_>, mutation: bool) -> Result<(), Error> {
    sessions::lock(tx, mutation)
        .await
        .map_err(|_| Error::Unavailable)
}
async fn authorize(
    tx: &mut Tx<'_>,
    actor: [u8; 32],
    mutation: bool,
) -> Result<(PrincipalId, u64), Error> {
    authority::actor(tx, actor, mutation)
        .await
        .map_err(|e| match e {
            darkhorse_domain::registration::RegistrationError::Unauthorized => Error::Unauthorized,
            darkhorse_domain::registration::RegistrationError::Forbidden => Error::Forbidden,
            darkhorse_domain::registration::RegistrationError::RecentAuthenticationRequired => {
                Error::RecentAuthentication
            }
            _ => Error::Unavailable,
        })
}
fn directory_error(e: darkhorse_application::directory::DirectoryFailure) -> Error {
    use darkhorse_application::directory::DirectoryFailure as F;
    match e {
        F::NotFound => Error::NotFound,
        F::Conflict => Error::Conflict,
        F::Policy(DirectoryError::LastAdministrator) => Error::LastAdministrator,
        F::Policy(_) => Error::Invalid,
        F::Unavailable => Error::Unavailable,
    }
}
fn storage<T>(_: T) -> Error {
    Error::Unavailable
}
fn uuid(id: u128) -> Uuid {
    Uuid::from_u128(id)
}
fn number(row: &PgRow, name: &str) -> Result<u64, Error> {
    row.try_get::<i64, _>(name)
        .map_err(storage)?
        .try_into()
        .map_err(storage)
}
