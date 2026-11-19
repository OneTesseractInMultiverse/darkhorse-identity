//! Bounded, confidential pre-login envelopes bound to issuer and browser proof.
mod wire;
use super::*;
use aws_lc_rs::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use zeroize::Zeroizing;

pub(super) const MAX_REFERENCE_BYTES: usize = 24_576;
const MAX_PLAINTEXT_BYTES: usize = 16_384;
pub struct ContinuationKey(LessSafeKey);
impl ContinuationKey {
    /// Derive a separate encryption key from the shared deployment admission key.
    pub fn derive(key: &[u8; 32]) -> Result<Self, Error> {
        let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(unavailable)?;
        mac.update(b"darkhorse:preauthorization:key:v1\0");
        let derived = Zeroizing::new(<[u8; 32]>::from(mac.finalize().into_bytes()));
        UnboundKey::new(&aead::AES_256_GCM, derived.as_ref())
            .map(LessSafeKey::new)
            .map(Self)
            .map_err(unavailable)
    }
    pub(super) fn seal(
        &self,
        pending: Preauthorization,
        issuer: &str,
        handle: &[u8; 32],
    ) -> Result<String, Error> {
        let mut nonce = [0; 12];
        getrandom::fill(&mut nonce).map_err(unavailable)?;
        let mut bytes = Zeroizing::new(wire::encode(pending)?);
        if bytes.len() > MAX_PLAINTEXT_BYTES {
            return Err(Error::InvalidRequest);
        }
        self.0
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(aad(issuer, handle)),
                &mut *bytes,
            )
            .map_err(unavailable)?;
        let mut envelope = nonce.to_vec();
        envelope.extend_from_slice(&bytes);
        Ok(URL_SAFE_NO_PAD.encode(envelope))
    }
    pub(super) fn open(
        &self,
        encoded: &str,
        issuer: &str,
        handle: &[u8; 32],
    ) -> Result<Preauthorization, Error> {
        if encoded.len() > MAX_REFERENCE_BYTES - 65 {
            return Err(Error::InvalidTransaction);
        }
        let mut bytes = Zeroizing::new(URL_SAFE_NO_PAD.decode(encoded).map_err(invalid)?);
        if bytes.len() < 28 || bytes.len() > MAX_PLAINTEXT_BYTES + 28 {
            return Err(Error::InvalidTransaction);
        }
        let nonce = bytes[..12].try_into().map_err(invalid)?;
        let plaintext = self
            .0
            .open_in_place(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(aad(issuer, handle)),
                &mut bytes[12..],
            )
            .map_err(invalid)?;
        wire::decode(plaintext)
    }
}
fn aad(issuer: &str, handle: &[u8; 32]) -> Vec<u8> {
    let mut bytes = b"darkhorse:preauthorization:v1\0".to_vec();
    bytes.extend_from_slice(issuer.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(handle);
    bytes
}
fn unavailable<T>(_: T) -> Error {
    Error::Unavailable
}
fn invalid<T>(_: T) -> Error {
    Error::InvalidTransaction
}
#[cfg(test)]
#[path = "../../tests/unit/provider_http/continuation.rs"]
mod tests;
