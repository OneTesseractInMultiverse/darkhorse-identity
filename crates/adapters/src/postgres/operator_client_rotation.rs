use super::{
    PostgresStore, operator_accounts,
    operator_mutations::{self, Mutation, Tx},
    registration, sessions,
};
use darkhorse_application::{
    operator_accounts::{CandidateAt, Store, Verified},
    registration::{Command, Entropy, NewSecret, Prepared, Record},
};
use darkhorse_domain::{
    identity::{ClientSecretId, OperationId},
    operator_accounts::Error,
    operator_client_rotation::Request,
    registration::RegistrationError,
};
use uuid::Uuid;
use zeroize::Zeroizing;

mod audit;

pub struct RotatedSecret {
    pub revision: u64,
    pub secret_id: ClientSecretId,
    pub secret: Zeroizing<String>,
}

pub struct Rotation<'a, E> {
    store: &'a PostgresStore,
    entropy: E,
}

impl PostgresStore {
    pub fn operator_client_rotation<E: Entropy>(&self, entropy: E) -> Rotation<'_, E> {
        Rotation {
            store: self,
            entropy,
        }
    }
}

impl<E: Entropy> Store for Rotation<'_, E> {
    type Request = Request;
    type Outcome = RotatedSecret;

    async fn candidate(&self, email: &str) -> Result<Option<CandidateAt>, Error> {
        Store::candidate(self.store, email).await
    }

    async fn denied(&self, id: OperationId, request: &Request) -> Result<(), Error> {
        operator_mutations::denied(self.store, id, request, self).await
    }

    async fn execute(&self, proof: Verified<Request>) -> Result<RotatedSecret, Error> {
        operator_mutations::execute(self.store, proof, self).await
    }
}

impl<E: Entropy> Mutation<Request> for Rotation<'_, E> {
    type Outcome = RotatedSecret;

    async fn mutate(
        &self,
        tx: &mut Tx<'_>,
        proof: &Verified<Request>,
    ) -> Result<RotatedSecret, Error> {
        let request = proof.request();
        let target = request.target();
        let command = Command::RotateSecret {
            application: target.application,
            client: target.client,
            revision: request.revision(),
            overlap_seconds: request.overlap_seconds(),
        };
        let now = sessions::now(tx).await.map_err(storage)?;
        registration::authority::command(tx, &command, now)
            .await
            .map_err(registration_error)?;
        operator_accounts::authority(tx, proof).await?;

        let NewSecret { value, verifier } = self.entropy.secret().map_err(registration_error)?;
        let secret_id = verifier.id;
        let secret = Zeroizing::new(value);
        let record = registration::write_current(
            tx,
            proof.candidate().credential.principal,
            &command,
            Prepared {
                identifier: None,
                secret: Some(verifier),
            },
            now,
        )
        .await
        .map_err(registration_error)?;
        operator_accounts::authority(tx, proof).await?;
        rotated(record, target.application, target.client, secret_id, secret)
    }

    async fn audit(
        &self,
        tx: &mut Tx<'_>,
        id: OperationId,
        request: &Request,
        actor: Option<&CandidateAt>,
        outcome: &Result<RotatedSecret, Error>,
    ) -> Result<(), Error> {
        audit::insert(tx, id, request, actor, outcome).await
    }
}

fn rotated(
    record: Record,
    application: darkhorse_domain::identity::ApplicationId,
    client: darkhorse_domain::identity::ClientId,
    secret_id: ClientSecretId,
    secret: Zeroizing<String>,
) -> Result<RotatedSecret, Error> {
    match record {
        Record::Client(record) if record.application == application && record.id == client => {
            Ok(RotatedSecret {
                revision: record.revision,
                secret_id,
                secret,
            })
        }
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
#[path = "../../tests/unit/postgres/operator_client_rotation.rs"]
mod tests;
