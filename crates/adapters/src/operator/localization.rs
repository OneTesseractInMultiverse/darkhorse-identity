//! Bounded operator presentation settings; never used as authority or command input.
use super::output::Failure;
pub use darkhorse_domain::localization::Locale;
use envbind::Environment;
pub(super) mod messages;

pub fn load(env: impl Environment) -> Result<Locale, Failure> {
    if let Some(value) = env
        .get("DARKHORSE_CLI_LOCALE")
        .map_err(|_| configuration())?
    {
        return crate::localization::parse(&value).map_err(|_| configuration());
    }
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Some(value) = env.get(key).map_err(|_| configuration())?
            && !value.is_empty()
        {
            return Ok(ambient(&value));
        }
    }
    Ok(Locale::English)
}
fn ambient(value: &str) -> Locale {
    if value.len() > 128 || !value.is_ascii() || value.chars().any(char::is_control) {
        return Locale::English;
    }
    let tag = value
        .split(['.', '@'])
        .next()
        .unwrap_or_default()
        .replace('_', "-");
    crate::locale_hint::ui_locales(Some(&tag))
        .ok()
        .flatten()
        .unwrap_or(Locale::English)
}
fn configuration() -> Failure {
    "Invalid operator language configuration; use en or es.".into()
}
pub(super) fn text(locale: Locale, english: &'static str) -> &'static str {
    match locale {
        Locale::English => english,
        Locale::Spanish => messages::spanish(english).unwrap_or(english),
    }
}
#[cfg(test)]
#[path = "../../tests/unit/operator/localization.rs"]
mod tests;
