use super::{
    PostgresStore, operator_accounts,
    operator_mutations::{self, Mutation, Tx},
    registration, sessions,
};
use darkhorse_application::{
    operator_accounts::{CandidateAt, Store, Verified},
    registration::{ClientRecord, Command, Entropy, NewSecret, Prepared, Record, SecretVerifier},
};
use darkhorse_domain::{
    identity::OperationId, operator_accounts::Error, operator_client_creation::Request,
    registration::RegistrationError,
};
use std::num::NonZeroU128;
use zeroize::Zeroizing;

mod audit;

pub(crate) struct CreatedClient {
    pub(crate) record: ClientRecord,
    pub(crate) secret_id: darkhorse_domain::identity::ClientSecretId,
    pub(crate) secret: Zeroizing<String>,
}

pub(crate) struct Creation<'a, E> {
    store: &'a PostgresStore,
    entropy: E,
}

impl PostgresStore {
    pub(crate) fn operator_client_creation<E: Entropy>(&self, entropy: E) -> Creation<'_, E> {
        Creation {
            store: self,
            entropy,
        }
    }
}

impl<E: Entropy> Store for Creation<'_, E> {
    type Request = Request;
    type Outcome = CreatedClient;

    async fn candidate(&self, email: &str) -> Result<Option<CandidateAt>, Error> {
        Store::candidate(self.store, email).await
    }

    async fn denied(&self, id: OperationId, request: &Request) -> Result<(), Error> {
        operator_mutations::denied(self.store, id, request, self).await
    }

    async fn execute(&self, proof: Verified<Request>) -> Result<CreatedClient, Error> {
        operator_mutations::execute(self.store, proof, self).await
    }
}

impl<E: Entropy> Mutation<Request> for Creation<'_, E> {
    type Outcome = CreatedClient;

    async fn mutate(
        &self,
        tx: &mut Tx<'_>,
        proof: &Verified<Request>,
    ) -> Result<CreatedClient, Error> {
        let request = proof.request();
        let command = command(request);
        let now = sessions::now(tx).await.map_err(storage)?;
        registration::authority::command(tx, &command, now)
            .await
            .map_err(registration_error)?;
        operator_accounts::authority(tx, proof).await?;

        let identifier = self.entropy.identifier().map_err(registration_error)?;
        let NewSecret { value, verifier } = self.entropy.secret().map_err(registration_error)?;
        let secret_id = verifier.id;
        let secret = Zeroizing::new(value);
        let record = registration::write_current(
            tx,
            proof.candidate().credential.principal,
            &command,
            prepared(identifier, verifier),
            now,
        )
        .await
        .map_err(registration_error)?;
        operator_accounts::authority(tx, proof).await?;
        Ok(CreatedClient {
            record: client(record)?,
            secret_id,
            secret,
        })
    }

    async fn audit(
        &self,
        tx: &mut Tx<'_>,
        id: OperationId,
        request: &Request,
        actor: Option<&CandidateAt>,
        outcome: &Result<CreatedClient, Error>,
    ) -> Result<(), Error> {
        audit::insert(tx, id, request, actor, outcome).await
    }
}

fn command(request: &Request) -> Command {
    Command::CreateClient {
        application: request.application(),
        spec: request.spec().clone(),
    }
}

fn prepared(identifier: NonZeroU128, secret: SecretVerifier) -> Prepared {
    Prepared {
        identifier: Some(identifier),
        secret: Some(secret),
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
#[path = "../../tests/unit/postgres/operator_client_creation.rs"]
mod tests;
