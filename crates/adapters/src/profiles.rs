//! Pinned, compiled country and phone metadata. No runtime files or remote lookups.
use darkhorse_domain::profiles::{Error, Fields, Input, Phone};
use rlibphonenumber::{PHONE_NUMBER_UTIL, PhoneNumberFormat};
pub const COUNTRY_VERSION: &str = "isocountry-0.3.2";
pub const PHONE_VERSION: &str = "libphonenumber-9.0.39";
pub fn countries() -> Vec<(&'static str, &'static str)> {
    isocountry::CountryCode::iter_alpha2()
        .map(|c| (c.alpha2(), c.name()))
        .collect()
}
pub fn calling_codes() -> Vec<u16> {
    PHONE_NUMBER_UTIL
        .get_supported_calling_codes()
        .filter_map(|code| u16::try_from(code).ok())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}
pub fn prepare(input: Input) -> Result<Fields, Error> {
    if let Some(phone) = &input.phone {
        validate_phone(phone)?;
    }
    stored_fields(input)
}
// Persisted contacts may predate today's numbering rules. Keep their structural
// validation, but use prepare for every new input at the transport boundary.
pub(crate) fn stored_fields(input: Input) -> Result<Fields, Error> {
    let codes = isocountry::CountryCode::iter_alpha2()
        .map(|c| c.alpha2())
        .collect::<Vec<_>>();
    Fields::new(input, &codes)
}
fn validate_phone(phone: &Phone) -> Result<(), Error> {
    let canonical = phone.e164();
    let number = PHONE_NUMBER_UTIL
        .parse(&canonical, None)
        .map_err(|_| Error::Invalid)?;
    if !number.is_valid()
        || number.extension.is_some()
        || number.country_code.to_string() != phone.calling_code()
        || number.format_as(PhoneNumberFormat::E164) != canonical
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
#[cfg(test)]
#[path = "../tests/unit/profiles.rs"]
mod tests;
