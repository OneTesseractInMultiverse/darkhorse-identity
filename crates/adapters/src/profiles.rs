//! Pinned, compiled country and phone metadata. No runtime files or remote lookups.
use darkhorse_domain::profiles::{Error, Fields, Input, Phone};
pub const COUNTRY_VERSION: &str = "isocountry-0.3.2";
pub const PHONE_VERSION: &str = "libphonenumber-9.0.33";
pub fn countries() -> Vec<(&'static str, &'static str)> {
    isocountry::CountryCode::iter_alpha2()
        .map(|c| (c.alpha2(), c.name()))
        .collect()
}
pub fn calling_codes() -> Vec<u16> {
    phonenumber::metadata::DATABASE
        .iter()
        .map(|m| m.country_code())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}
pub fn prepare(input: Input) -> Result<Fields, Error> {
    if let Some(phone) = &input.phone {
        validate_phone(phone)?;
    }
    let codes = isocountry::CountryCode::iter_alpha2()
        .map(|c| c.alpha2())
        .collect::<Vec<_>>();
    Fields::new(input, &codes)
}
fn validate_phone(phone: &Phone) -> Result<(), Error> {
    let number = phonenumber::parse(None, phone.e164()).map_err(|_| Error::Invalid)?;
    if !phonenumber::is_valid(&number)
        || number.extension().is_some()
        || number.format().mode(phonenumber::Mode::E164).to_string() != phone.e164()
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
#[cfg(test)]
#[path = "../tests/unit/profiles.rs"]
mod tests;
