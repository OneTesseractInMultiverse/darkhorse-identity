//! Ordered, bounded protocol language hints; never identity or authorization authority.
use crate::localization::InvalidLocale;
use darkhorse_domain::localization::Locale;
/// Missing or syntactically valid unsupported tags select no transaction override.
/// All entries are validated, including those after the first supported language.
pub fn ui_locales(value: Option<&str>) -> Result<Option<Locale>, InvalidLocale> {
    let Some(value) = value else { return Ok(None) };
    if value.is_empty() || value.len() > 256 || !value.is_ascii() {
        return Err(InvalidLocale);
    }
    let mut selected = None;
    for (index, tag) in value.split(' ').enumerate() {
        if index >= 16
            || tag.is_empty()
            || tag.len() > 64
            || tag.split('-').any(|part| part.is_empty() || part.len() > 8)
        {
            return Err(InvalidLocale);
        }
        let tag = language_tags::LanguageTag::parse(tag).map_err(|_| InvalidLocale)?;
        let locale = crate::localization::parse(&tag.primary_language().to_ascii_lowercase()).ok();
        selected = selected.or(locale);
    }
    Ok(selected)
}
#[cfg(test)]
#[path = "../tests/unit/locale_hint.rs"]
mod tests;
