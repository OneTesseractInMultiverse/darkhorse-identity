use super::{PostgresStore, operator_accounts, registration, sessions};
use darkhorse_application::{
    operator_accounts::{CandidateAt, Store, Verified},
    registration::{ApplicationRecord, Command, Entropy, Prepared, Record},
};
use darkhorse_domain::{
    identity::OperationId,
    operator_accounts::Error,
    operator_applications::{Operation, Request},
    registration::RegistrationError,
};
use sqlx::{Acquire, Postgres, Transaction};
mod audit;
type Tx<'a> = Transaction<'a, Postgres>;

pub struct Applications<'a, E> {
    store: &'a PostgresStore,
    entropy: E,
}
impl PostgresStore {
    pub fn operator_applications<E: Entropy>(&self, entropy: E) -> Applications<'_, E> {
        Applications {
            store: self,
            entropy,
        }
    }
}
impl<E: Entropy> Store for Applications<'_, E> {
    type Request = Request;
    type Outcome = ApplicationRecord;
    async fn candidate(&self, email: &str) -> Result<Option<CandidateAt>, Error> {
        Store::candidate(self.store, email).await
    }
    async fn denied(&self, id: OperationId, request: &Request) -> Result<(), Error> {
        let mut tx = self.store.pool.begin().await.map_err(storage)?;
        audit::insert(&mut tx, id, request, None, &Err(Error::Denied)).await?;
        tx.commit().await.map_err(|_| Error::Uncertain)
    }
    async fn execute(&self, proof: Verified<Request>) -> Result<ApplicationRecord, Error> {
        let mut tx = self.store.pool.begin().await.map_err(storage)?;
        sessions::lock(&mut tx, true).await.map_err(storage)?;
        let result = execute(&mut tx, &proof, &self.entropy).await;
        if matches!(result, Err(Error::Unavailable)) {
            return result;
        }
        audit::insert(
            &mut tx,
            proof.id(),
            proof.request(),
            Some(proof.candidate()),
            &result,
        )
        .await?;
        if result.is_ok() {
            operator_accounts::authority(&mut tx, &proof).await?;
        }
        tx.commit().await.map_err(|_| Error::Uncertain)?;
        result
    }
}
async fn execute(
    tx: &mut Tx<'_>,
    proof: &Verified<Request>,
    entropy: &impl Entropy,
) -> Result<ApplicationRecord, Error> {
    operator_accounts::authority(tx, proof).await?;
    let mut savepoint = tx.begin().await.map_err(storage)?;
    let result = mutate(&mut savepoint, proof, entropy).await;
    if result.is_ok() {
        savepoint.commit().await.map_err(storage)?;
    } else {
        savepoint.rollback().await.map_err(storage)?;
    }
    result
}
async fn mutate(
    tx: &mut Tx<'_>,
    proof: &Verified<Request>,
    entropy: &impl Entropy,
) -> Result<ApplicationRecord, Error> {
    let command = command(proof.request().operation());
    let now = sessions::now(tx).await.map_err(storage)?;
    registration::authority::command(tx, &command, now)
        .await
        .map_err(registration_error)?;
    operator_accounts::authority(tx, proof).await?;
    let prepared = prepare(&command, entropy)?;
    let record = registration::write_current(
        tx,
        proof.candidate().credential.principal,
        &command,
        prepared,
        now,
    )
    .await
    .map_err(registration_error)?;
    operator_accounts::authority(tx, proof).await?;
    application(record)
}
fn application(record: Record) -> Result<ApplicationRecord, Error> {
    match record {
        Record::Application(app) => Ok(app),
        _ => Err(Error::Unavailable),
    }
}
fn command(operation: &Operation) -> Command {
    match operation {
        Operation::Create(spec) => Command::CreateApplication(spec.clone()),
        Operation::Update {
            application,
            revision,
            spec,
        } => Command::UpdateApplication {
            application: *application,
            revision: *revision,
            spec: spec.clone(),
        },
    }
}
fn prepare(command: &Command, entropy: &impl Entropy) -> Result<Prepared, Error> {
    let identifier = command
        .needs_identifier()
        .then(|| entropy.identifier())
        .transpose()
        .map_err(registration_error)?;
    Ok(Prepared {
        identifier,
        secret: None,
    })
}
fn registration_error(error: RegistrationError) -> Error {
    match error {
        RegistrationError::Invalid => Error::Invalid,
        RegistrationError::NotFound => Error::NotFound,
        RegistrationError::Conflict => Error::Conflict,
        RegistrationError::Unauthorized
        | RegistrationError::Forbidden
        | RegistrationError::RecentAuthenticationRequired => Error::Denied,
        RegistrationError::Unavailable => Error::Unavailable,
    }
}
fn storage<T>(_: T) -> Error {
    Error::Unavailable
}
#[cfg(test)]
#[path = "../../tests/unit/postgres/operator_applications.rs"]
mod tests;
