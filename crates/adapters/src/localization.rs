//! Canonical stored/configured language values; regional protocol hints are separate.
use darkhorse_domain::localization::Locale;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidLocale;
pub fn parse(value: &str) -> Result<Locale, InvalidLocale> {
    match value {
        "en" => Ok(Locale::English),
        "es" => Ok(Locale::Spanish),
        _ => Err(InvalidLocale),
    }
}
pub fn tag(locale: Locale) -> &'static str {
    match locale {
        Locale::English => "en",
        Locale::Spanish => "es",
    }
}
