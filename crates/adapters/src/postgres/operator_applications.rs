use super::{
    PostgresStore, operator_accounts,
    operator_mutations::{self, Mutation, Tx},
    registration, sessions,
};
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
mod audit;

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
        operator_mutations::denied(self.store, id, request, self).await
    }
    async fn execute(&self, proof: Verified<Request>) -> Result<ApplicationRecord, Error> {
        operator_mutations::execute(self.store, proof, self).await
    }
}
impl<E: Entropy> Mutation<Request> for Applications<'_, E> {
    type Outcome = ApplicationRecord;
    async fn mutate(
        &self,
        tx: &mut Tx<'_>,
        proof: &Verified<Request>,
    ) -> Result<ApplicationRecord, Error> {
        mutate(tx, proof, &self.entropy).await
    }
    async fn audit(
        &self,
        tx: &mut Tx<'_>,
        id: OperationId,
        request: &Request,
        actor: Option<&CandidateAt>,
        outcome: &Result<ApplicationRecord, Error>,
    ) -> Result<(), Error> {
        audit::insert(tx, id, request, actor, outcome).await
    }
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
