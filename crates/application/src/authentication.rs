//! Authentication sequencing. Persistence revalidates credentials atomically at issuance.
use darkhorse_domain::{
    authentication::{authenticated, login_email, login_password},
    identity::{CredentialId, PrincipalId},
};
use std::future::Future;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    Denied,
    Unavailable,
    Limited { retry_after_ms: u32 },
}

// Credential and bearer containers deliberately do not implement Debug.
pub struct Candidate {
    pub principal: PrincipalId,
    pub credential: CredentialId,
    pub epoch: u64,
    pub active: bool,
    pub verifier: String,
}
pub struct SessionSecret {
    pub value: String,
    pub digest: [u8; 32],
}
#[derive(Debug, PartialEq, Eq)]
pub struct SessionView {
    pub principal: PrincipalId,
    pub name: String,
    pub locale: Option<darkhorse_domain::localization::Locale>,
}
pub struct SignedIn {
    pub secret: SessionSecret,
    pub view: SessionView,
}

pub struct Service<S, L, P, E> {
    pub store: S,
    pub admission: L,
    pub passwords: P,
    pub entropy: E,
}
pub trait BrowserAuthentication: Send + Sync {
    fn login(
        &self,
        email: &str,
        password: &str,
        previous: Option<[u8; 32]>,
    ) -> impl Future<Output = Result<SignedIn, AuthError>> + Send;
    fn session(
        &self,
        digest: [u8; 32],
    ) -> impl Future<Output = Result<SessionView, AuthError>> + Send;
    fn logout(&self, digest: [u8; 32]) -> impl Future<Output = Result<(), AuthError>> + Send;
}
impl<S, L, P, E> BrowserAuthentication for Service<S, L, P, E>
where
    S: AuthenticationStore + Send + Sync,
    L: LoginAdmission + Send + Sync,
    P: PasswordVerification + Send + Sync,
    E: SessionEntropy + Send + Sync,
{
    async fn login(
        &self,
        email: &str,
        password: &str,
        previous: Option<[u8; 32]>,
    ) -> Result<SignedIn, AuthError> {
        login(
            &self.store,
            &self.admission,
            &self.passwords,
            &self.entropy,
            email,
            password,
            previous,
        )
        .await
    }
    async fn session(&self, digest: [u8; 32]) -> Result<SessionView, AuthError> {
        self.store.session(digest).await
    }
    async fn logout(&self, digest: [u8; 32]) -> Result<(), AuthError> {
        self.store.logout(digest).await
    }
}

pub trait LoginAdmission {
    fn admit(&self, email_key: &str) -> impl Future<Output = Result<(), AuthError>> + Send;
}
pub trait PasswordVerification {
    /// None must perform equivalent bounded work using a dummy verifier.
    fn verify(
        &self,
        password: &str,
        verifier: Option<&str>,
    ) -> impl Future<Output = Result<bool, AuthError>> + Send;
}
pub trait SessionEntropy {
    fn generate(&self) -> Result<SessionSecret, AuthError>;
}
pub trait AuthenticationStore {
    fn candidate(
        &self,
        email_key: &str,
    ) -> impl Future<Output = Result<Option<Candidate>, AuthError>> + Send;
    /// Recheck exact credential, verifier, epoch, active status under locks. Replace
    /// a known existing session atomically; a concurrent replacement has one winner.
    fn establish(
        &self,
        candidate: &Candidate,
        digest: [u8; 32],
        previous: Option<[u8; 32]>,
    ) -> impl Future<Output = Result<SessionView, AuthError>> + Send;
    /// Read current primary state on every check; never reuse an earlier decision.
    fn session(
        &self,
        digest: [u8; 32],
    ) -> impl Future<Output = Result<SessionView, AuthError>> + Send;
    fn logout(&self, digest: [u8; 32]) -> impl Future<Output = Result<(), AuthError>> + Send;
}

pub async fn login(
    store: &impl AuthenticationStore,
    admission: &impl LoginAdmission,
    passwords: &impl PasswordVerification,
    entropy: &impl SessionEntropy,
    email: &str,
    password: &str,
    previous: Option<[u8; 32]>,
) -> Result<SignedIn, AuthError> {
    let email_key = login_email(email).map_err(|_| AuthError::Denied)?;
    login_password(password).map_err(|_| AuthError::Denied)?;
    admission.admit(&email_key).await?;
    let candidate = store.candidate(&email_key).await?;
    let matches = passwords
        .verify(password, candidate.as_ref().map(|c| c.verifier.as_str()))
        .await?;
    let candidate = verified_candidate(candidate, matches)?;
    let secret = entropy.generate()?;
    let view = store.establish(&candidate, secret.digest, previous).await?;
    Ok(SignedIn { secret, view })
}
fn verified_candidate(candidate: Option<Candidate>, matches: bool) -> Result<Candidate, AuthError> {
    if !authenticated(
        candidate.is_some(),
        candidate.as_ref().is_some_and(|c| c.active),
        matches,
    ) {
        return Err(AuthError::Denied);
    }
    candidate.ok_or(AuthError::Denied)
}

#[cfg(test)]
#[path = "../tests/unit/authentication.rs"]
mod tests;
