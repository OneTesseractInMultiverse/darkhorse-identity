use darkhorse_domain::signing::KeyError;
use envbind::{Binder, BoolVar, Environment, ParameterSource, StringVar};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;
pub struct WrapKey(Zeroizing<[u8; 32]>);
impl WrapKey {
    pub fn from_hex(value: &str) -> Result<Self, KeyError> {
        let key = crate::session_secret::decode(value).map_err(|_| KeyError::Invalid)?;
        if key == [0; 32] {
            return Err(KeyError::Invalid);
        }
        Ok(Self(Zeroizing::new(key)))
    }
    pub fn fingerprint(&self) -> [u8; 32] {
        Sha256::new()
            .chain_update(b"darkhorse:signing-wrap:v1\0")
            .chain_update(self.0.as_slice())
            .finalize()
            .into()
    }
    pub(super) fn bytes(&self) -> &[u8] {
        self.0.as_slice()
    }
}
struct Raw {
    enabled: bool,
    key: String,
}
impl ParameterSource for Raw {
    fn bind<E: Environment>(b: &Binder<E>) -> Result<Self, envbind::BindError> {
        Ok(Self {
            enabled: b.bind(&BoolVar::new("DARKHORSE_PROVIDER_ENABLED").default(false))?,
            key: b.bind(
                &StringVar::new("DARKHORSE_SIGNING_WRAP_KEY")
                    .default("")
                    .max_bytes(64),
            )?,
        })
    }
}
pub fn load(environment: impl Environment) -> Result<Option<WrapKey>, KeyError> {
    let raw = Raw::from_environment(environment).map_err(|_| KeyError::Invalid)?;
    if !raw.enabled {
        return Ok(None);
    }
    WrapKey::from_hex(&Zeroizing::new(raw.key)).map(Some)
}
#[cfg(test)]
#[path = "../../tests/unit/signing/configuration.rs"]
mod tests;
