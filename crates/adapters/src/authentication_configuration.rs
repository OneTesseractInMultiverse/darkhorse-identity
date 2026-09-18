use envbind::{Binder, BoolVar, Environment, ParameterSource, StringVar};
pub struct AuthenticationSettings {
    pub key: [u8; 32],
}
impl AuthenticationSettings {
    pub fn key_digest(&self) -> [u8; 32] {
        use sha2::Digest;
        sha2::Sha256::digest(self.key).into()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthenticationConfigurationError;
struct Raw {
    enabled: bool,
    key: String,
}
impl ParameterSource for Raw {
    fn bind<E: Environment>(binder: &Binder<E>) -> Result<Self, envbind::BindError> {
        Ok(Self {
            enabled: binder.bind(&BoolVar::new("DARKHORSE_LOGIN_ENABLED").default(false))?,
            key: binder.bind(
                &StringVar::new("DARKHORSE_LOGIN_LIMIT_KEY")
                    .default("")
                    .max_bytes(64),
            )?,
        })
    }
}
pub fn load(
    environment: impl Environment,
) -> Result<Option<AuthenticationSettings>, AuthenticationConfigurationError> {
    validate(Raw::from_environment(environment).map_err(|_| AuthenticationConfigurationError)?)
}
fn validate(raw: Raw) -> Result<Option<AuthenticationSettings>, AuthenticationConfigurationError> {
    if !raw.enabled {
        return Ok(None);
    }
    let key =
        crate::session_secret::decode(&raw.key).map_err(|_| AuthenticationConfigurationError)?;
    if key == [0; 32] {
        return Err(AuthenticationConfigurationError);
    }
    Ok(Some(AuthenticationSettings { key }))
}
#[cfg(test)]
#[path = "../tests/unit/authentication_configuration.rs"]
mod tests;
