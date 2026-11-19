//! Password replacement preparation. Authorization and persistence belong to callers.
use crate::authentication::AuthError;
use darkhorse_domain::identity::CredentialId;
use std::future::Future;

// Deliberately lacks Debug: password verifiers must not enter diagnostics.
pub struct PreparedPasswordReplacement {
    pub credential_id: CredentialId,
    pub verifier: String,
}

pub trait PasswordReplacementPreparation: Send + Sync {
    /// Prepare an independently salted verifier and credential ID without creating
    /// an account identity. The caller validates password policy; preserve exact bytes.
    fn prepare_replacement(
        &self,
        password: &str,
    ) -> impl Future<Output = Result<PreparedPasswordReplacement, AuthError>> + Send;
}
