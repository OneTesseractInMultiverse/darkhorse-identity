use super::{
    PostgresStore, operator_accounts,
    operator_mutations::{self, Mutation, Tx},
    registration, sessions,
};
use darkhorse_application::{
    operator_accounts::{CandidateAt, Store, Verified},
    registration::{ClientRecord, Command, Record},
};
use darkhorse_domain::{
    identity::OperationId, operator_accounts::Error, operator_clients::Request,
    registration::RegistrationError,
};
mod audit;

pub struct Clients<'a>(&'a PostgresStore);
impl PostgresStore {
    pub fn operator_clients(&self) -> Clients<'_> {
        Clients(self)
    }
}
impl Store for Clients<'_> {
    type Request = Request;
    type Outcome = ClientRecord;
    async fn candidate(&self, email: &str) -> Result<Option<CandidateAt>, Error> {
        Store::candidate(self.0, email).await
    }
    async fn denied(&self, id: OperationId, request: &Request) -> Result<(), Error> {
        operator_mutations::denied(self.0, id, request, self).await
    }
    async fn execute(&self, proof: Verified<Request>) -> Result<ClientRecord, Error> {
        operator_mutations::execute(self.0, proof, self).await
    }
}
impl Mutation<Request> for Clients<'_> {
    type Outcome = ClientRecord;
    async fn mutate(
        &self,
        tx: &mut Tx<'_>,
        proof: &Verified<Request>,
    ) -> Result<ClientRecord, Error> {
        let command = command(proof.request());
        let now = sessions::now(tx).await.map_err(storage)?;
        registration::authority::command(tx, &command, now)
            .await
            .map_err(registration_error)?;
        operator_accounts::authority(tx, proof).await?;
        let record = registration::write_configuration_current(
            tx,
            proof.candidate().credential.principal,
            &command,
            now,
        )
        .await
        .map_err(registration_error)?;
        operator_accounts::authority(tx, proof).await?;
        client(record)
    }
    async fn audit(
        &self,
        tx: &mut Tx<'_>,
        id: OperationId,
        request: &Request,
        actor: Option<&CandidateAt>,
        outcome: &Result<ClientRecord, Error>,
    ) -> Result<(), Error> {
        audit::insert(tx, id, request, actor, outcome).await
    }
}
fn command(request: &Request) -> Command {
    let u = request.update();
    Command::UpdateClient {
        application: u.application,
        client: u.client,
        revision: u.revision,
        spec: u.spec.clone(),
    }
}
fn client(record: Record) -> Result<ClientRecord, Error> {
    match record {
        Record::Client(record) => Ok(record),
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
#[path = "../../tests/unit/postgres/operator_clients.rs"]
mod tests;
