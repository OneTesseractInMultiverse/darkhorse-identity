use darkhorse_domain::profiles::Error;
use envbind::{Binder, BoolVar, Environment, ParameterSource, StringVar};
use zeroize::Zeroizing;
pub struct Settings {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key: Zeroizing<String>,
    pub secret_key: Zeroizing<String>,
    pub insecure: bool,
}
struct Raw {
    enabled: bool,
    endpoint: String,
    bucket: String,
    region: String,
    access: String,
    secret: String,
    insecure: bool,
}
impl ParameterSource for Raw {
    fn bind<E: Environment>(b: &Binder<E>) -> Result<Self, envbind::BindError> {
        Ok(Self {
            enabled: b.bind(&BoolVar::new("DARKHORSE_OBJECTS_ENABLED").default(false))?,
            endpoint: b.bind(
                &StringVar::new("DARKHORSE_OBJECTS_ENDPOINT")
                    .default("")
                    .max_bytes(2048),
            )?,
            bucket: b.bind(
                &StringVar::new("DARKHORSE_OBJECTS_BUCKET")
                    .default("")
                    .max_bytes(63),
            )?,
            region: b.bind(
                &StringVar::new("DARKHORSE_OBJECTS_REGION")
                    .default("us-east-1")
                    .max_bytes(100),
            )?,
            access: b.bind(
                &StringVar::new("DARKHORSE_OBJECTS_ACCESS_KEY")
                    .default("")
                    .max_bytes(256),
            )?,
            secret: b.bind(
                &StringVar::new("DARKHORSE_OBJECTS_SECRET_KEY")
                    .default("")
                    .max_bytes(4096),
            )?,
            insecure: b.bind(&BoolVar::new("DARKHORSE_OBJECTS_LOCAL_HTTP").default(false))?,
        })
    }
}
pub fn load(env: impl Environment) -> Result<Option<Settings>, Error> {
    validate(Raw::from_environment(env).map_err(|_| Error::Invalid)?)
}
fn validate(raw: Raw) -> Result<Option<Settings>, Error> {
    let access_key = Zeroizing::new(raw.access);
    let secret_key = Zeroizing::new(raw.secret);
    if !raw.enabled {
        return Ok(None);
    }
    let endpoint = url::Url::parse(&raw.endpoint).map_err(|_| Error::Invalid)?;
    if endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
        || endpoint.path() != "/"
        || access_key.is_empty()
        || secret_key.is_empty()
        || access_key.chars().any(char::is_control)
        || secret_key.chars().any(char::is_control)
        || raw.region.is_empty()
        || !raw
            .region
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        || !(3..=63).contains(&raw.bucket.len())
        || !raw
            .bucket
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        || raw.bucket.starts_with('-')
        || raw.bucket.ends_with('-')
    {
        return Err(Error::Invalid);
    }
    let local = raw.insecure
        && endpoint.scheme() == "http"
        && matches!(
            endpoint.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]")
        );
    if endpoint.scheme() != "https" && !local {
        return Err(Error::Invalid);
    }
    Ok(Some(Settings {
        endpoint: endpoint.to_string(),
        bucket: raw.bucket,
        region: raw.region,
        access_key,
        secret_key,
        insecure: local,
    }))
}
#[cfg(test)]
#[path = "../../tests/unit/media/configuration.rs"]
mod tests;
