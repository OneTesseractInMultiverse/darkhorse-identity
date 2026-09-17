//! Validated, explicit Redis settings; parsing does not connect or read files.
use envbind::{Binder, BoolVar, Environment, ParameterSource, StringVar, U16Var};

pub struct Endpoint {
    pub(crate) url: url::Url,
    pub(crate) connections: u16,
    pub(crate) timeout_ms: u16,
}
pub struct RedisSettings {
    pub(crate) cache: Endpoint,
    pub(crate) limiter: Endpoint,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RedisConfigurationError;
struct Raw {
    cache: String,
    limiter: String,
    cache_connections: u16,
    limiter_connections: u16,
    timeout_ms: u16,
    insecure: bool,
}
impl ParameterSource for Raw {
    fn bind<E: Environment>(binder: &Binder<E>) -> Result<Self, envbind::BindError> {
        Ok(Self {
            cache: binder.bind(&StringVar::new("DARKHORSE_REDIS_CACHE_URL").max_bytes(4096))?,
            limiter: binder.bind(&StringVar::new("DARKHORSE_REDIS_LIMITER_URL").max_bytes(4096))?,
            cache_connections: binder
                .bind(&U16Var::new("DARKHORSE_REDIS_CACHE_CONNECTIONS").default(2))?,
            limiter_connections: binder
                .bind(&U16Var::new("DARKHORSE_REDIS_LIMITER_CONNECTIONS").default(4))?,
            timeout_ms: binder.bind(&U16Var::new("DARKHORSE_REDIS_TIMEOUT_MS").default(250))?,
            insecure: binder.bind(&BoolVar::new("DARKHORSE_REDIS_INSECURE").default(false))?,
        })
    }
}
pub fn load(environment: impl Environment) -> Result<RedisSettings, RedisConfigurationError> {
    validate(Raw::from_environment(environment).map_err(|_| RedisConfigurationError)?)
}
fn validate(raw: Raw) -> Result<RedisSettings, RedisConfigurationError> {
    let cache = endpoint(
        &raw.cache,
        raw.cache_connections,
        raw.timeout_ms,
        raw.insecure,
    )?;
    let limiter = endpoint(
        &raw.limiter,
        raw.limiter_connections,
        raw.timeout_ms,
        raw.insecure,
    )?;
    if (
        cache.url.host_str().map(str::to_ascii_lowercase),
        cache.url.port().unwrap_or(6379),
    ) == (
        limiter.url.host_str().map(str::to_ascii_lowercase),
        limiter.url.port().unwrap_or(6379),
    ) || password(&cache.url)? == password(&limiter.url)?
    {
        return Err(RedisConfigurationError);
    }
    Ok(RedisSettings { cache, limiter })
}
fn endpoint(
    raw: &str,
    connections: u16,
    timeout_ms: u16,
    insecure: bool,
) -> Result<Endpoint, RedisConfigurationError> {
    let url = url::Url::parse(raw).map_err(|_| RedisConfigurationError)?;
    if !(url.scheme() == "rediss" || (insecure && url.scheme() == "redis"))
        || url.host_str().is_none_or(|h| h.contains('%'))
        || url.port() == Some(0)
        || !matches!(url.path(), "" | "/" | "/0")
        || url.query().is_some()
        || url.fragment().is_some()
        || url.username().is_empty()
        || url.username() == "default"
        || url.username().len() > 64
        || !url
            .username()
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
        || !(1..=16).contains(&connections)
        || !(10..=1000).contains(&timeout_ms)
    {
        return Err(RedisConfigurationError);
    }
    password(&url)?;
    Ok(Endpoint {
        url,
        connections,
        timeout_ms,
    })
}
fn password(url: &url::Url) -> Result<std::borrow::Cow<'_, str>, RedisConfigurationError> {
    let raw = url
        .password()
        .filter(|p| !p.is_empty())
        .ok_or(RedisConfigurationError)?;
    percent_encoding::percent_decode_str(raw)
        .decode_utf8()
        .map_err(|_| RedisConfigurationError)
}
#[cfg(test)]
#[path = "../tests/unit/redis_configuration.rs"]
mod tests;
