use super::network::Policy;
use envbind::{Binder, Environment, ParameterSource, StringVar};

pub struct Settings {
    pub(super) policy: Policy,
    pub(super) certificates: Vec<reqwest::Certificate>,
}
#[derive(Debug)]
pub struct ConfigurationError;
struct Raw {
    allowlist: String,
    ca: String,
}
impl ParameterSource for Raw {
    fn bind<E: Environment>(binder: &Binder<E>) -> Result<Self, envbind::BindError> {
        Ok(Self {
            allowlist: binder.bind(
                &StringVar::new("DARKHORSE_LOGOUT_PRIVATE_DESTINATIONS")
                    .default("{}")
                    .max_bytes(16384),
            )?,
            ca: binder.bind(
                &StringVar::new("DARKHORSE_LOGOUT_CA_PEM")
                    .default("")
                    .max_bytes(32768),
            )?,
        })
    }
}
pub fn load(environment: impl Environment) -> Result<Settings, ConfigurationError> {
    validate(Raw::from_environment(environment).map_err(|_| ConfigurationError)?)
}
fn validate(raw: Raw) -> Result<Settings, ConfigurationError> {
    Ok(Settings {
        policy: Policy::parse(&raw.allowlist).map_err(|_| ConfigurationError)?,
        certificates: certificates(&raw.ca)?,
    })
}
fn certificates(pem: &str) -> Result<Vec<reqwest::Certificate>, ConfigurationError> {
    if pem.is_empty() {
        return Ok(Vec::new());
    }
    let certs =
        reqwest::Certificate::from_pem_bundle(pem.as_bytes()).map_err(|_| ConfigurationError)?;
    if certs.is_empty() || certs.len() > 8 {
        return Err(ConfigurationError);
    }
    Ok(certs)
}

#[cfg(test)]
#[path = "../../tests/unit/logout/configuration.rs"]
mod tests;
