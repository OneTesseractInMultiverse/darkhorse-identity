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
        self.purpose_token(seed, b"darkhorse:email-verification:v1\0", "ev1_")
    }
    pub fn invitation_token(&self, seed: [u8; 32]) -> Zeroizing<String> {
        self.purpose_token(seed, b"darkhorse:invitation:v1\0", "iv1_")
    }
    fn purpose_token(&self, seed: [u8; 32], label: &[u8], prefix: &str) -> Zeroizing<String> {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(self.0.as_slice()).expect("HMAC accepts a 32-byte key");
        mac.update(label);
        mac.update(&seed);
        Zeroizing::new(format!(
            "{prefix}{}",
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

impl darkhorse_application::invitations::InvitationSecrets for Secrets {
    fn issue_invitation(
        &self,
    ) -> Result<darkhorse_application::invitations::Material, darkhorse_domain::invitations::Error>
    {
        use darkhorse_domain::{identity::InvitationId, invitations::Error};
        let mut seed = [0; 32];
        getrandom::fill(&mut seed).map_err(|_| Error::Unavailable)?;
        let id = InvitationId::from_u128(uuid::Uuid::new_v4().as_u128())
            .map_err(|_| Error::Unavailable)?;
        let digest = invitation_digest(&self.invitation_token(seed))?;
        Ok(darkhorse_application::invitations::Material { id, seed, digest })
    }
}
pub fn invitation_digest(value: &str) -> Result<[u8; 32], darkhorse_domain::invitations::Error> {
    use darkhorse_domain::invitations::Error;
    let token = value.strip_prefix("iv1_").ok_or(Error::Invalid)?;
    crate::session_secret::decode(token).map_err(|_| Error::Invalid)?;
    Ok(Sha256::digest(value.as_bytes()).into())
}
