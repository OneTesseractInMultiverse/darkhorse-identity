use darkhorse_application::tokens::Opaque;
use darkhorse_domain::tokens::Error;
use sha2::{Digest, Sha256};
#[derive(Clone, Copy)]
pub enum Purpose {
    Code,
    Access,
    Refresh,
}
impl Purpose {
    fn prefix(self) -> &'static str {
        match self {
            Self::Code => "dc_",
            Self::Access => "da_",
            Self::Refresh => "dr_",
        }
    }
}
pub fn pair() -> Result<darkhorse_application::tokens::Material, Error> {
    Ok(darkhorse_application::tokens::Material {
        access: generate(Purpose::Access)?,
        refresh: generate(Purpose::Refresh)?,
    })
}
pub fn generate(purpose: Purpose) -> Result<Opaque, Error> {
    let mut bytes = zeroize::Zeroizing::new([0; 32]);
    getrandom::fill(bytes.as_mut()).map_err(|_| Error::Unavailable)?;
    let value = format!(
        "{}{}",
        purpose.prefix(),
        crate::session_secret::hex(&*bytes)
    );
    let digest = digest(&value, purpose)?;
    Ok(Opaque { value, digest })
}
pub fn digest(value: &str, purpose: Purpose) -> Result<[u8; 32], Error> {
    let raw = value
        .strip_prefix(purpose.prefix())
        .ok_or(Error::InvalidGrant)?;
    crate::session_secret::decode(raw).map_err(|_| Error::InvalidGrant)?;
    Ok(Sha256::new()
        .chain_update(b"darkhorse:oauth-credential:v1\0")
        .chain_update(value.as_bytes())
        .finalize()
        .into())
}
pub fn challenge(verifier: &str) -> Result<[u8; 32], Error> {
    if !(43..=128).contains(&verifier.len())
        || !verifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
    {
        return Err(Error::InvalidGrant);
    }
    Ok(Sha256::digest(verifier.as_bytes()).into())
}
#[cfg(test)]
#[path = "../../tests/unit/tokens/material.rs"]
mod tests;
