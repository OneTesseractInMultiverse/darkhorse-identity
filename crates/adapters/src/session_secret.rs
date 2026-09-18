//! High-entropy browser handles; raw values are never persistence identifiers.
use darkhorse_application::authentication::{AuthError, SessionEntropy, SessionSecret};
use sha2::{Digest, Sha256};
pub struct OsSessionEntropy;
impl SessionEntropy for OsSessionEntropy {
    fn generate(&self) -> Result<SessionSecret, AuthError> {
        let mut bytes = [0; 32];
        getrandom::fill(&mut bytes).map_err(|_| AuthError::Unavailable)?;
        Ok(from_bytes(bytes))
    }
}
fn from_bytes(bytes: [u8; 32]) -> SessionSecret {
    let value = hex(&bytes);
    let digest = Sha256::digest(value.as_bytes()).into();
    SessionSecret { value, digest }
}
pub(crate) fn digest(value: &str) -> Result<[u8; 32], AuthError> {
    decode(value)?;
    Ok(Sha256::digest(value.as_bytes()).into())
}
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
pub(crate) fn decode(value: &str) -> Result<[u8; 32], AuthError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(AuthError::Denied);
    }
    let mut bytes = [0; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| AuthError::Denied)?;
    }
    Ok(bytes)
}
#[cfg(test)]
#[path = "../tests/unit/session_secret.rs"]
mod tests;
