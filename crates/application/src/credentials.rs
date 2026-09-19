//! Prepared password credential data shared by account creation use cases.
use darkhorse_domain::identity::{CredentialId, PrincipalId};
// Deliberately lacks Debug: password verifiers must not enter diagnostics.
pub struct PreparedCredential {
    pub principal_id: PrincipalId,
    pub credential_id: CredentialId,
    pub verifier: String,
}
