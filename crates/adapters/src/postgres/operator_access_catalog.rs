//! Fresh-password operator writes to the shared access catalog.
use super::{
    PostgresStore, admin_catalog, operator_accounts,
    operator_mutations::{self, Mutation, Tx},
};
use darkhorse_application::{
    admin_catalog::{Target, Written},
    operator_accounts::{CandidateAt, Store, Verified},
    registration::Entropy,
};
use darkhorse_domain::{
    admin_catalog::Change, identity::OperationId, operator_accounts::Error,
    operator_catalog::MutationRequest, registration::RegistrationError,
};
mod audit;

pub struct AccessCatalog<'a, E> {
    store: &'a PostgresStore,
    entropy: E,
}

impl PostgresStore {
    pub fn operator_access_catalog<E: Entropy>(&self, entropy: E) -> AccessCatalog<'_, E> {
        AccessCatalog {
            store: self,
            entropy,
        }
    }
}

impl<E: Entropy> Store for AccessCatalog<'_, E> {
    type Request = MutationRequest;
    type Outcome = Written;

    async fn candidate(&self, email: &str) -> Result<Option<CandidateAt>, Error> {
        Store::candidate(self.store, email).await
    }

    async fn denied(&self, id: OperationId, request: &MutationRequest) -> Result<(), Error> {
        operator_mutations::denied(self.store, id, request, self).await
    }

    async fn execute(&self, proof: Verified<MutationRequest>) -> Result<Written, Error> {
        operator_mutations::execute(self.store, proof, self).await
    }
}

impl<E: Entropy> Mutation<MutationRequest> for AccessCatalog<'_, E> {
    type Outcome = Written;

    async fn mutate(
        &self,
        tx: &mut Tx<'_>,
        proof: &Verified<MutationRequest>,
    ) -> Result<Written, Error> {
        operator_accounts::authority(tx, proof).await?;
        admin_catalog::validate_operator_revision(tx, proof.request().policy_revision())
            .await
            .map_err(registration_error)?;
        let identifier = new_identifier(proof.request().change(), &self.entropy)?;
        let written = admin_catalog::operator_write(
            tx,
            proof.request().policy_revision(),
            proof.request().change(),
            identifier,
        )
        .await
        .map_err(registration_error)?;
        operator_accounts::authority(tx, proof).await?;
        Ok(written)
    }

    async fn audit(
        &self,
        tx: &mut Tx<'_>,
        id: OperationId,
        request: &MutationRequest,
        actor: Option<&CandidateAt>,
        outcome: &Result<Written, Error>,
    ) -> Result<(), Error> {
        audit::insert(tx, id, request, actor, outcome).await
    }
}

fn new_identifier(
    change: &Change,
    entropy: &impl Entropy,
) -> Result<Option<std::num::NonZeroU128>, Error> {
    if change.needs_identifier() {
        entropy.identifier().map(Some).map_err(registration_error)
    } else {
        Ok(None)
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

#[cfg(test)]
#[path = "../../tests/unit/postgres/operator_access_catalog.rs"]
mod tests;
