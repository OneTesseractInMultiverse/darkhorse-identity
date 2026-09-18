//! Entropy and purpose-separated verifier preparation for resource credentials.
use darkhorse_application::{registration::Entropy as RegistrationEntropy, resource_servers::*};
use darkhorse_domain::{identity::CredentialId, registration::RegistrationError};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;
pub struct OsResourceEntropy;
impl Entropy for OsResourceEntropy {
    fn secret(&self) -> Result<NewSecret, RegistrationError> {
        let id = CredentialId::from_u128(
            crate::registration::OsRegistrationEntropy
                .identifier()?
                .get(),
        )
        .map_err(|_| RegistrationError::Unavailable)?;
        let mut bytes = Zeroizing::new([0; 32]);
        getrandom::fill(bytes.as_mut()).map_err(|_| RegistrationError::Unavailable)?;
        Ok(from_bytes(*bytes, id))
    }
}
fn from_bytes(bytes: [u8; 32], id: CredentialId) -> NewSecret {
    let value = crate::session_secret::hex(&bytes);
    NewSecret {
        verifier: Verifier {
            id,
            digest: hash(&value),
        },
        value,
    }
}
fn hash(value: &str) -> [u8; 32] {
    Sha256::new()
        .chain_update(b"darkhorse:resource-secret:v1\0")
        .chain_update(value.as_bytes())
        .finalize()
        .into()
}
pub fn secret_digest(value: &str) -> Result<[u8; 32], RegistrationError> {
    crate::session_secret::decode(value).map_err(|_| RegistrationError::Unauthorized)?;
    Ok(hash(value))
}
#[cfg(test)]
#[path = "../tests/unit/resource_servers.rs"]
mod tests;
