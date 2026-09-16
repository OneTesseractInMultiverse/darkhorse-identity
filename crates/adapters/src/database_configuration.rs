use envbind::{Binder, BoolVar, Environment, ParameterSource, StringVar, U16Var};
pub struct DatabaseSettings {
    pub url: url::Url,
    pub insecure: bool,
    pub max_connections: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatabaseConfigurationError;

struct Raw {
    url: String,
    max_connections: u16,
    insecure: bool,
}
impl ParameterSource for Raw {
    fn bind<E: Environment>(binder: &Binder<E>) -> Result<Self, envbind::BindError> {
        Ok(Self {
            url: binder.bind(&StringVar::new("DARKHORSE_DATABASE_URL").max_bytes(4096))?,
            max_connections: binder
                .bind(&U16Var::new("DARKHORSE_DATABASE_POOL_SIZE").default(5))?,
            insecure: binder.bind(&BoolVar::new("DARKHORSE_DATABASE_INSECURE").default(false))?,
        })
    }
}
pub fn load(environment: impl Environment) -> Result<DatabaseSettings, DatabaseConfigurationError> {
    let raw = Raw::from_environment(environment).map_err(|_| DatabaseConfigurationError)?;
    validate(raw)
}
fn validate(raw: Raw) -> Result<DatabaseSettings, DatabaseConfigurationError> {
    let parsed = url::Url::parse(&raw.url).map_err(|_| DatabaseConfigurationError)?;
    if !matches!(parsed.scheme(), "postgres" | "postgresql")
        || parsed.host_str().is_none()
        || parsed.host_str().is_some_and(|host| host.contains('%'))
        || parsed.username().is_empty()
        || parsed.password().is_none_or(str::is_empty)
        || parsed.path().len() <= 1
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !(1..=32).contains(&raw.max_connections)
    {
        return Err(DatabaseConfigurationError);
    }
    Ok(DatabaseSettings {
        url: parsed,
        insecure: raw.insecure,
        max_connections: raw.max_connections.into(),
    })
}

#[cfg(test)]
#[path = "../tests/unit/database_configuration.rs"]
mod tests;
