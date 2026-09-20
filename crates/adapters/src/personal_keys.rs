//! OS randomness and domain-separated verifiers for opaque personal credentials.
use darkhorse_application::personal_keys::{Entropy, Prepared, Verifier};
use darkhorse_domain::{identity::CredentialId, personal_keys::Error};
use sha2::{Digest, Sha256};
pub struct OsKeyEntropy;
impl Entropy for OsKeyEntropy {
    fn key(&self) -> Result<Prepared, Error> {
        let mut secret = zeroize::Zeroizing::new([0; 32]);
        let mut id = [0; 16];
        getrandom::fill(secret.as_mut()).map_err(|_| Error::Unavailable)?;
        getrandom::fill(&mut id).map_err(|_| Error::Unavailable)?;
        prepare(id, &secret)
    }
}
fn prepare(id: [u8; 16], secret: &[u8; 32]) -> Result<Prepared, Error> {
    let id = CredentialId::from_u128(u128::from_be_bytes(id)).map_err(|_| Error::Unavailable)?;
    let value = format!("dk_{}", crate::session_secret::hex(secret));
    Ok(Prepared {
        verifier: Verifier {
            id,
            digest: digest(&value)?,
        },
        value,
    })
}
pub fn digest(value: &str) -> Result<[u8; 32], Error> {
    let raw = value.strip_prefix("dk_").ok_or(Error::Invalid)?;
    crate::session_secret::decode(raw).map_err(|_| Error::Invalid)?;
    Ok(Sha256::new()
        .chain_update(b"darkhorse:personal-key:v1\0")
        .chain_update(value.as_bytes())
        .finalize()
        .into())
}
#[cfg(test)]
#[path = "../tests/unit/personal_keys.rs"]
mod tests;
