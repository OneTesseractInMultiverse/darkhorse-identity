use super::*;
use envbind::{Binder, BoolVar, Environment, ParameterSource, StringVar, U16Var};
use lettre::Address;
pub struct Settings {
    pub secrets: Secrets,
    pub host: String,
    pub port: u16,
    pub from: Address,
    pub username: String,
    pub password: Zeroizing<String>,
    pub ca_file: Option<String>,
}
struct Raw {
    enabled: bool,
    key: String,
    host: String,
    port: u16,
    from: String,
    username: String,
    password: String,
    ca_file: String,
}
impl ParameterSource for Raw {
    fn bind<E: Environment>(b: &Binder<E>) -> Result<Self, envbind::BindError> {
        Ok(Self {
            enabled: b.bind(&BoolVar::new("DARKHORSE_EMAIL_ENABLED").default(false))?,
            key: b.bind(
                &StringVar::new("DARKHORSE_EMAIL_KEY")
                    .default("")
                    .max_bytes(64),
            )?,
            host: b.bind(
                &StringVar::new("DARKHORSE_SMTP_HOST")
                    .default("")
                    .max_bytes(253),
            )?,
            port: b.bind(&U16Var::new("DARKHORSE_SMTP_PORT").default(465))?,
            from: b.bind(
                &StringVar::new("DARKHORSE_SMTP_FROM")
                    .default("")
                    .max_bytes(254),
            )?,
            username: b.bind(
                &StringVar::new("DARKHORSE_SMTP_USERNAME")
                    .default("")
                    .max_bytes(1024),
            )?,
            password: b.bind(
                &StringVar::new("DARKHORSE_SMTP_PASSWORD")
                    .default("")
                    .max_bytes(4096),
            )?,
            ca_file: b.bind(
                &StringVar::new("DARKHORSE_SMTP_CA_FILE")
                    .default("")
                    .max_bytes(4096),
            )?,
        })
    }
}
pub fn load(env: impl Environment) -> Result<Option<Settings>, Error> {
    let raw = Raw::from_environment(env).map_err(|_| Error::Invalid)?;
    validate(raw)
}
fn validate(raw: Raw) -> Result<Option<Settings>, Error> {
    let key = Zeroizing::new(raw.key);
    let password = Zeroizing::new(raw.password);
    if !raw.enabled {
        return Ok(None);
    }
    valid_host(&raw.host)?;
    if raw.port == 0
        || raw.username.is_empty() != password.is_empty()
        || raw.username.chars().any(char::is_control)
        || raw.from.chars().any(char::is_control)
    {
        return Err(Error::Invalid);
    }
    Ok(Some(Settings {
        secrets: Secrets::from_hex(&key)?,
        host: raw.host,
        port: raw.port,
        from: raw.from.parse().map_err(|_| Error::Invalid)?,
        username: raw.username,
        password,
        ca_file: (!raw.ca_file.is_empty()).then_some(raw.ca_file),
    }))
}
fn valid_host(host: &str) -> Result<(), Error> {
    if host.is_empty()
        || !host.is_ascii()
        || host.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
#[cfg(test)]
#[path = "../../tests/unit/email_verification/configuration.rs"]
mod tests;
