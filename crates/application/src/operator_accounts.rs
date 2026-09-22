//! One password proof, one account operation, no reusable administrative session.
use crate::{
    authentication::{AuthError, Candidate, LoginAdmission, PasswordVerification},
    directory::AccountRecord,
};
use darkhorse_domain::{
    authentication::{authenticated, login_email, login_password},
    identity::OperationId,
    operator_accounts::{Error, Request},
};
use std::future::Future;

pub struct CandidateAt {
    pub credential: Candidate,
    pub observed_ms: u64,
}
/// Only successful password verification constructs this nonserializable proof.
/// Persistence must still revalidate its credential, age and current grants.
pub struct Verified {
    candidate: CandidateAt,
    id: OperationId,
    request: Request,
}
impl Verified {
    pub fn candidate(&self) -> &CandidateAt {
        &self.candidate
    }
    pub fn id(&self) -> OperationId {
        self.id
    }
    pub fn request(&self) -> &Request {
        &self.request
    }
}
pub struct Outcome {
    pub account: AccountRecord,
    pub changed: bool,
}
pub trait Store: Sync {
    fn candidate(
        &self,
        email: &str,
    ) -> impl Future<Output = Result<Option<CandidateAt>, Error>> + Send;
    fn denied(
        &self,
        id: OperationId,
        request: &Request,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn execute(&self, proof: Verified) -> impl Future<Output = Result<Outcome, Error>> + Send;
}

pub async fn run(
    store: &impl Store,
    admission: &impl LoginAdmission,
    passwords: &impl PasswordVerification,
    id: OperationId,
    request: Request,
    email: &str,
    password: &str,
) -> Result<Outcome, Error> {
    let email = login_email(email).map_err(|_| Error::Denied)?;
    login_password(password).map_err(|_| Error::Denied)?;
    admission.admit(&email).await.map_err(auth_error)?;
    let candidate = store.candidate(&email).await?;
    let matches = passwords
        .verify(
            password,
            candidate.as_ref().map(|c| c.credential.verifier.as_str()),
        )
        .await
        .map_err(auth_error)?;
    match verified(candidate, matches) {
        Ok(candidate) => {
            store
                .execute(Verified {
                    candidate,
                    id,
                    request,
                })
                .await
        }
        Err(error) => {
            store.denied(id, &request).await?;
            Err(error)
        }
    }
}
fn verified(candidate: Option<CandidateAt>, matches: bool) -> Result<CandidateAt, Error> {
    if !authenticated(
        candidate.is_some(),
        candidate.as_ref().is_some_and(|c| c.credential.active),
        matches,
    ) {
        return Err(Error::Denied);
    }
    candidate.ok_or(Error::Denied)
}
fn auth_error(error: AuthError) -> Error {
    match error {
        AuthError::Denied => Error::Denied,
        AuthError::Unavailable => Error::Unavailable,
        AuthError::Limited { retry_after_ms } => Error::Limited { retry_after_ms },
    }
}

#[cfg(test)]
#[path = "../tests/unit/operator_accounts.rs"]
mod tests;
