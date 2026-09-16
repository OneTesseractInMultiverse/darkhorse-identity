//! Operator bootstrap contracts. The store owns one atomic database operation.
use darkhorse_domain::{
    directory::{DirectoryError, Profile, validate_password},
    identity::{CredentialId, PrincipalId},
};
use std::future::Future;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapError {
    Invalid(DirectoryError),
    SecretPreparation,
    AlreadyInitialized,
    Storage,
}

pub struct BootstrapRequest<'a> {
    pub email: &'a str,
    pub first_name: &'a str,
    pub last_name: &'a str,
    pub password: &'a str,
}

// Deliberately lacks Debug: password verifiers must not enter diagnostics.
pub struct PreparedCredential {
    pub principal_id: PrincipalId,
    pub credential_id: CredentialId,
    pub verifier: String,
}
pub struct NewAdministrator {
    pub profile: Profile,
    pub credential: PreparedCredential,
}

pub trait CredentialPreparation {
    fn prepare(
        &self,
        password: &str,
    ) -> impl Future<Output = Result<PreparedCredential, BootstrapError>> + Send;
}

pub trait BootstrapStore {
    /// Atomically reject repeat bootstrap or insert principal, credential,
    /// platform-administrator membership, audit, and consumed bootstrap state.
    fn bootstrap(
        &self,
        administrator: NewAdministrator,
    ) -> impl Future<Output = Result<PrincipalId, BootstrapError>> + Send;
}

pub async fn bootstrap(
    store: &impl BootstrapStore,
    preparation: &impl CredentialPreparation,
    request: BootstrapRequest<'_>,
) -> Result<PrincipalId, BootstrapError> {
    let profile = Profile::new(request.email, request.first_name, request.last_name)
        .map_err(BootstrapError::Invalid)?;
    validate_password(request.password).map_err(BootstrapError::Invalid)?;
    let credential = preparation.prepare(request.password).await?;
    store
        .bootstrap(NewAdministrator {
            profile,
            credential,
        })
        .await
}

#[cfg(test)]
#[path = "../tests/unit/bootstrap.rs"]
mod tests;
