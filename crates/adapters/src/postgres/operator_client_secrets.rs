use super::{
    PostgresStore, operator_accounts,
    operator_mutations::{self, Mutation, Tx},
    registration, sessions,
};
use darkhorse_application::{
    operator_accounts::{CandidateAt, Store, Verified},
    operator_client_secrets::{Inventory, Metadata, Outcome},
    registration::{Command, Record},
};
use darkhorse_domain::{
    identity::{ClientSecretId, OperationId},
    operator_accounts::{Error, needs_current_authority},
    operator_client_secrets::{Operation, Request, Target},
    registration::RegistrationError,
};
use uuid::Uuid;
mod audit;
mod inventory;
pub struct ClientSecrets<'a>(&'a PostgresStore);
impl PostgresStore {
    pub fn operator_client_secrets(&self) -> ClientSecrets<'_> {
        ClientSecrets(self)
    }
}
impl Store for ClientSecrets<'_> {
    type Request = Request;
    type Outcome = Outcome;
    async fn candidate(&self, email: &str) -> Result<Option<CandidateAt>, Error> {
        Store::candidate(self.0, email).await
    }
    async fn denied(&self, id: OperationId, request: &Request) -> Result<(), Error> {
        operator_mutations::denied(self.0, id, request, self).await
    }
    async fn execute(&self, proof: Verified<Request>) -> Result<Outcome, Error> {
        match proof.request().operation() {
            Operation::List { .. } => self.read(proof).await,
            Operation::Retire { .. } => operator_mutations::execute(self.0, proof, self).await,
        }
    }
}
impl ClientSecrets<'_> {
    async fn read(&self, proof: Verified<Request>) -> Result<Outcome, Error> {
        let mut tx = self.0.pool.begin().await.map_err(storage)?;
        sessions::lock(&mut tx, false).await.map_err(storage)?;
        let result = read(&mut tx, &proof).await;
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
        if needs_current_authority(&result) {
            operator_accounts::authority(&mut tx, &proof).await?;
        }
        tx.commit().await.map_err(|_| Error::Uncertain)?;
        result
    }
}
async fn read(tx: &mut Tx<'_>, proof: &Verified<Request>) -> Result<Outcome, Error> {
    operator_accounts::authority(tx, proof).await?;
    let result = inventory::read(tx, proof.request()).await;
    operator_accounts::authority(tx, proof).await?;
    result.map(Outcome::Listed)
}
impl Mutation<Request> for ClientSecrets<'_> {
    type Outcome = Outcome;
    async fn mutate(&self, tx: &mut Tx<'_>, proof: &Verified<Request>) -> Result<Outcome, Error> {
        let command = retirement(proof.request())?;
        let now = sessions::now(tx).await.map_err(storage)?;
        registration::authority::command(tx, &command, now)
            .await
            .map_err(registration_error)?;
        operator_accounts::authority(tx, proof).await?;
        let record =
            registration::retire_current(tx, proof.candidate().credential.principal, &command, now)
                .await
                .map_err(registration_error)?;
        operator_accounts::authority(tx, proof).await?;
        retired(record)
    }
    async fn audit(
        &self,
        tx: &mut Tx<'_>,
        id: OperationId,
        request: &Request,
        actor: Option<&CandidateAt>,
        outcome: &Result<Outcome, Error>,
    ) -> Result<(), Error> {
        audit::insert(tx, id, request, actor, outcome).await
    }
}
fn retirement(request: &Request) -> Result<Command, Error> {
    let Operation::Retire { secret, revision } = request.operation() else {
        return Err(Error::Invalid);
    };
    let Target {
        application,
        client,
    } = request.target();
    Ok(Command::RetireSecret {
        application,
        client,
        secret,
        revision,
    })
}
fn retired(record: Record) -> Result<Outcome, Error> {
    match record {
        Record::Client(c) => Ok(Outcome::Retired {
            revision: c.revision,
        }),
        _ => Err(Error::Unavailable),
    }
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
#[path = "../../tests/unit/postgres/operator_client_secrets.rs"]
mod tests;
