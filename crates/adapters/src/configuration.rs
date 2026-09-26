//! Startup binding is separate from deterministic settings validation.

use envbind::{Binder, Environment, ParameterSource, StringVar, U16Var};
use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
};
use url::Url;

#[derive(Debug, PartialEq, Eq)]
pub struct HttpSettings {
    pub listen: SocketAddr,
    pub public_origin: Url,
    pub static_dir: PathBuf,
    pub default_locale: darkhorse_domain::localization::Locale,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigurationError {
    Binding,
    ListenAddress,
    PublicOrigin,
    StaticDirectory,
    DefaultLocale,
}

struct RawSettings {
    host: String,
    port: u16,
    public_origin: String,
    static_dir: String,
    locale: String,
}

impl ParameterSource for RawSettings {
    fn bind<E: Environment>(binder: &Binder<E>) -> Result<Self, envbind::BindError> {
        Ok(Self {
            locale: binder.bind(
                &StringVar::new("DARKHORSE_DEFAULT_LOCALE")
                    .allow_empty()
                    .default("en")
                    .max_bytes(2),
            )?,
            host: binder.bind(
                &StringVar::new("DARKHORSE_HTTP_HOST")
                    .default("127.0.0.1")
                    .max_bytes(128),
            )?,
            port: binder.bind(&U16Var::new("DARKHORSE_HTTP_PORT").default(3001))?,
            public_origin: binder.bind(
                &StringVar::new("DARKHORSE_PUBLIC_ORIGIN")
                    .default("https://localhost:8443")
                    .max_bytes(2048),
            )?,
            static_dir: binder.bind(
                &StringVar::new("DARKHORSE_STATIC_DIR")
                    .default("apps/console/build")
                    .max_bytes(4096),
            )?,
        })
    }
}

pub fn load(environment: impl Environment) -> Result<HttpSettings, ConfigurationError> {
    let raw =
        RawSettings::from_environment(environment).map_err(|_| ConfigurationError::Binding)?;
    validate(raw)
}

fn validate(raw: RawSettings) -> Result<HttpSettings, ConfigurationError> {
    let host = raw
        .host
        .parse::<IpAddr>()
        .map_err(|_| ConfigurationError::ListenAddress)?;
    if raw.port == 0 {
        return Err(ConfigurationError::ListenAddress);
    }
    let public_origin = validate_origin(&raw.public_origin)?;
    if raw.static_dir.trim().is_empty() {
        return Err(ConfigurationError::StaticDirectory);
    }
    Ok(HttpSettings {
        listen: SocketAddr::new(host, raw.port),
        public_origin,
        static_dir: raw.static_dir.into(),
        default_locale: crate::localization::parse(&raw.locale)
            .map_err(|_| ConfigurationError::DefaultLocale)?,
    })
}

fn validate_origin(raw: &str) -> Result<Url, ConfigurationError> {
    let origin = Url::parse(raw).map_err(|_| ConfigurationError::PublicOrigin)?;
    if origin.scheme() != "https"
        || origin.host_str().is_none()
        || !origin.username().is_empty()
        || origin.password().is_some()
        || origin.path() != "/"
        || origin.query().is_some()
        || origin.fragment().is_some()
        || origin.port() == Some(0)
    {
        return Err(ConfigurationError::PublicOrigin);
    }
    Ok(origin)
}

#[cfg(test)]
#[path = "../tests/unit/configuration.rs"]
mod tests;
