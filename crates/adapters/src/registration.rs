//! URL profile and cryptographic preparation at the infrastructure boundary.
use darkhorse_application::registration::{Entropy, NewSecret, SecretVerifier};
use darkhorse_domain::{
    identity::ClientSecretId,
    registration::{Redirects, RegistrationError},
};
use sha2::{Digest, Sha256};
use std::num::NonZeroU128;
use zeroize::Zeroizing;

pub struct OsRegistrationEntropy;
impl Entropy for OsRegistrationEntropy {
    fn identifier(&self) -> Result<NonZeroU128, RegistrationError> {
        let mut bytes = [0; 16];
        getrandom::fill(&mut bytes).map_err(|_| RegistrationError::Unavailable)?;
        NonZeroU128::new(
            uuid::Builder::from_random_bytes(bytes)
                .into_uuid()
                .as_u128(),
        )
        .ok_or(RegistrationError::Unavailable)
    }
    fn secret(&self) -> Result<NewSecret, RegistrationError> {
        let id = ClientSecretId::from_u128(self.identifier()?.get())
            .map_err(|_| RegistrationError::Unavailable)?;
        let mut bytes = Zeroizing::new([0; 32]);
        getrandom::fill(bytes.as_mut()).map_err(|_| RegistrationError::Unavailable)?;
        Ok(from_bytes(*bytes, id))
    }
}
fn from_bytes(bytes: [u8; 32], id: ClientSecretId) -> NewSecret {
    let value = crate::session_secret::hex(&bytes);
    NewSecret {
        verifier: SecretVerifier {
            id,
            digest: hash(&value),
        },
        value,
    }
}
fn hash(value: &str) -> [u8; 32] {
    Sha256::new()
        .chain_update(b"darkhorse:client-secret:v1\0")
        .chain_update(value.as_bytes())
        .finalize()
        .into()
}
pub fn secret_digest(value: &str) -> Result<[u8; 32], RegistrationError> {
    crate::session_secret::decode(value).map_err(|_| RegistrationError::Unauthorized)?;
    Ok(hash(value))
}
pub fn redirects(values: Vec<String>) -> Result<Redirects, RegistrationError> {
    if values.iter().any(|value| !valid_callback(value)) {
        return Err(RegistrationError::Invalid);
    }
    Redirects::from_validated_urls(values)
}
fn valid_callback(value: &str) -> bool {
    if !value.is_ascii()
        || value
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control() || b == b'\\' || b == b'*')
        || !valid_escapes(value)
    {
        return false;
    }
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    url.scheme() == "https"
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
        && url.as_str() == value
}
fn valid_escapes(value: &str) -> bool {
    value.as_bytes().iter().enumerate().all(|(i, byte)| {
        *byte != b'%'
            || value
                .as_bytes()
                .get(i + 1..i + 3)
                .is_some_and(|pair| pair.iter().all(u8::is_ascii_hexdigit))
    })
}
#[cfg(test)]
#[path = "../tests/unit/registration.rs"]
mod tests;

mod client_input;
pub(crate) use client_input::ClientInput;
