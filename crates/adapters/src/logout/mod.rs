pub mod configuration;
mod network;
pub mod sender;

use darkhorse_domain::registration::RegistrationError;
pub fn destination(value: &str) -> Result<url::Url, RegistrationError> {
    let parsed = url::Url::parse(value).map_err(|_| RegistrationError::Invalid)?;
    if value.len() > 2048
        || value.chars().any(char::is_control)
        || parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
        || parsed.as_str() != value
    {
        return Err(RegistrationError::Invalid);
    }
    Ok(parsed)
}

#[cfg(test)]
#[path = "../../tests/unit/logout/destination.rs"]
mod tests;
