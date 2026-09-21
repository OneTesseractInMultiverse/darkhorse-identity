//! File-backed secrets are read at configuration boundaries without mutating process state.
use envbind::{Environment, EnvironmentError, ProcessEnvironment};
use std::{fs::File, io::Read, path::Path};
use zeroize::Zeroizing;

const MAX_BYTES: usize = 32768;
const FILE_SETTINGS: &[&str] = &[
    "DARKHORSE_EMAIL_KEY",
    "DARKHORSE_SMTP_USERNAME",
    "DARKHORSE_SMTP_PASSWORD",
    "DARKHORSE_DATABASE_URL",
    "DARKHORSE_LOGIN_LIMIT_KEY",
    "DARKHORSE_SIGNING_WRAP_KEY",
    "DARKHORSE_REDIS_CACHE_URL",
    "DARKHORSE_REDIS_LIMITER_URL",
    "DARKHORSE_REDIS_CACHE_ADMIN_URL",
    "DARKHORSE_REDIS_LIMITER_ADMIN_URL",
    "DARKHORSE_REDIS_CACHE_CA_PEM",
    "DARKHORSE_REDIS_LIMITER_CA_PEM",
    "DARKHORSE_OBJECTS_ACCESS_KEY",
    "DARKHORSE_OBJECTS_SECRET_KEY",
];

#[derive(Clone, Copy)]
pub struct DeploymentEnvironment;
impl Environment for DeploymentEnvironment {
    fn get(&self, name: &str) -> Result<Option<String>, EnvironmentError> {
        resolve(&ProcessEnvironment, &Files, name)
    }
}
trait SecretFiles {
    fn read(&self, path: &str) -> Result<Vec<u8>, EnvironmentError>;
}
struct Files;
impl SecretFiles for Files {
    fn read(&self, path: &str) -> Result<Vec<u8>, EnvironmentError> {
        let metadata = std::fs::metadata(path).map_err(|_| unavailable())?;
        regular(&metadata)?;
        let file = File::open(path).map_err(|_| unavailable())?;
        regular(&file.metadata().map_err(|_| unavailable())?)?;
        let mut bytes = Zeroizing::new(Vec::new());
        file.take((MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| unavailable())?;
        Ok(std::mem::take(&mut *bytes))
    }
}
fn regular(metadata: &std::fs::Metadata) -> Result<(), EnvironmentError> {
    use std::os::unix::fs::PermissionsExt;
    if !metadata.is_file()
        || metadata.len() > MAX_BYTES as u64
        || metadata.permissions().mode() & 0o022 != 0
    {
        return Err(unavailable());
    }
    Ok(())
}
enum Source {
    Direct(Option<String>),
    File(String),
}
fn source(value: Option<String>, path: Option<String>) -> Result<Source, EnvironmentError> {
    match (value, path) {
        (Some(_), Some(_)) => Err(unavailable()),
        (value, None) => Ok(Source::Direct(value)),
        (None, Some(path)) => {
            if path.len() > 4096
                || !Path::new(&path).is_absolute()
                || path.chars().any(char::is_control)
            {
                return Err(unavailable());
            }
            Ok(Source::File(path))
        }
    }
}
fn resolve(
    env: &impl Environment,
    files: &impl SecretFiles,
    name: &str,
) -> Result<Option<String>, EnvironmentError> {
    let value = env.get(name)?;
    if !FILE_SETTINGS.contains(&name) {
        return Ok(value);
    }
    match source(value, env.get(&format!("{name}_FILE"))?)? {
        Source::Direct(value) => Ok(value),
        Source::File(path) => decode(files.read(&path)?).map(Some),
    }
}
fn decode(bytes: Vec<u8>) -> Result<String, EnvironmentError> {
    let bytes = Zeroizing::new(bytes);
    if bytes.is_empty() || bytes.len() > MAX_BYTES || bytes.contains(&0) {
        return Err(unavailable());
    }
    let value = std::str::from_utf8(&bytes).map_err(|_| unavailable())?;
    let value = value
        .strip_suffix("\r\n")
        .or_else(|| value.strip_suffix('\n'))
        .unwrap_or(value);
    if value.is_empty() {
        return Err(unavailable());
    }
    Ok(value.to_owned())
}
fn unavailable() -> EnvironmentError {
    EnvironmentError::read("Secret configuration is unavailable or ambiguous.")
}

#[cfg(test)]
#[path = "../tests/unit/deployment_environment.rs"]
mod tests;
