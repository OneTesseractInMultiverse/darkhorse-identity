//! Purpose-bound verification secrets and TLS mail transport.
use darkhorse_application::email_verification::{Material, VerificationSecrets};
use darkhorse_domain::{email_verification::Error, identity::EmailVerificationId};
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use zeroize::Zeroizing;
pub mod configuration;
pub mod smtp;
#[derive(Clone)]
pub struct Secrets(Arc<Zeroizing<[u8; 32]>>);
impl Secrets {
    pub fn from_hex(value: &str) -> Result<Self, Error> {
        let key = crate::session_secret::decode(value).map_err(|_| Error::Invalid)?;
        if key == [0; 32] {
            return Err(Error::Invalid);
        }
        Ok(Self(Arc::new(Zeroizing::new(key))))
    }
    pub fn fingerprint(&self) -> [u8; 32] {
        Sha256::new()
            .chain_update(b"darkhorse:email-verification-key:v1\0")
            .chain_update(self.0.as_slice())
            .finalize()
            .into()
    }
    pub fn token(&self, seed: [u8; 32]) -> Zeroizing<String> {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(self.0.as_slice()).expect("HMAC accepts a 32-byte key");
        mac.update(b"darkhorse:email-verification:v1\0");
        mac.update(&seed);
        Zeroizing::new(format!(
            "ev1_{}",
            crate::session_secret::hex(&mac.finalize().into_bytes())
        ))
    }
}
impl VerificationSecrets for Secrets {
    fn issue(&self) -> Result<Material, Error> {
        let mut seed = [0; 32];
        getrandom::fill(&mut seed).map_err(|_| Error::Unavailable)?;
        let id = EmailVerificationId::from_u128(uuid::Uuid::new_v4().as_u128())
            .map_err(|_| Error::Unavailable)?;
        let digest = token_digest(&self.token(seed))?;
        Ok(Material { id, seed, digest })
    }
}
pub fn token_digest(value: &str) -> Result<[u8; 32], Error> {
    let token = value.strip_prefix("ev1_").ok_or(Error::Invalid)?;
    crate::session_secret::decode(token).map_err(|_| Error::Invalid)?;
    Ok(Sha256::digest(value.as_bytes()).into())
}
#[cfg(test)]
#[path = "../../tests/unit/email_verification/mod.rs"]
mod tests;
