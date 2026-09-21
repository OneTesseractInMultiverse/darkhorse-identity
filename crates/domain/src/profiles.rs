//! Descriptive attributes are validated independently from credentials and authority.
use crate::{directory::validate_name, identity::PrincipalId};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Unauthorized,
    Forbidden,
    RecentAuthentication,
    NotFound,
    Conflict,
    Unavailable,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phone {
    calling_code: String,
    national_number: String,
}
impl Phone {
    pub fn new(code: &str, number: &str) -> Result<Self, Error> {
        if !(1..=3).contains(&code.len())
            || code.starts_with('0')
            || !code.bytes().all(|b| b.is_ascii_digit())
            || number.is_empty()
            || code.len() + number.len() > 15
            || !number.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            calling_code: code.into(),
            national_number: number.into(),
        })
    }
    pub fn calling_code(&self) -> &str {
        &self.calling_code
    }
    pub fn national_number(&self) -> &str {
        &self.national_number
    }
    pub fn e164(&self) -> String {
        format!("+{}{}", self.calling_code, self.national_number)
    }
}
#[derive(Debug, Clone)]
pub struct Input {
    pub first_name: String,
    pub second_name: String,
    pub last_name: String,
    pub second_last_name: String,
    pub country: String,
    pub bio: String,
    pub phone: Option<Phone>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fields {
    first_name: String,
    second_name: Option<String>,
    last_name: String,
    second_last_name: Option<String>,
    country: Option<String>,
    bio: Option<String>,
    phone: Option<Phone>,
}
impl Fields {
    pub fn new(input: Input, countries: &[&str]) -> Result<Self, Error> {
        if !input.country.is_empty()
            && (!countries.contains(&input.country.as_str())
                || input.country.len() != 2
                || !input.country.bytes().all(|c| c.is_ascii_uppercase()))
        {
            return Err(Error::Invalid);
        }
        if input.bio.chars().count() > 2000
            || input.bio.len() > 8000
            || input
                .bio
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            first_name: validate_name(&input.first_name).map_err(|_| Error::Invalid)?,
            last_name: validate_name(&input.last_name).map_err(|_| Error::Invalid)?,
            second_name: optional_name(&input.second_name)?,
            second_last_name: optional_name(&input.second_last_name)?,
            country: optional(input.country),
            bio: optional(input.bio),
            phone: input.phone,
        })
    }
    pub fn first_name(&self) -> &str {
        &self.first_name
    }
    pub fn second_name(&self) -> Option<&str> {
        self.second_name.as_deref()
    }
    pub fn last_name(&self) -> &str {
        &self.last_name
    }
    pub fn second_last_name(&self) -> Option<&str> {
        self.second_last_name.as_deref()
    }
    pub fn country(&self) -> Option<&str> {
        self.country.as_deref()
    }
    pub fn bio(&self) -> Option<&str> {
        self.bio.as_deref()
    }
    pub fn phone(&self) -> Option<&Phone> {
        self.phone.as_ref()
    }
}
fn optional(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}
fn optional_name(value: &str) -> Result<Option<String>, Error> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        validate_name(value).map(Some).map_err(|_| Error::Invalid)
    }
}
pub fn authorize(
    actor: PrincipalId,
    target: PrincipalId,
    administrator: bool,
) -> Result<(), Error> {
    if actor == target || administrator {
        Ok(())
    } else {
        Err(Error::Forbidden)
    }
}
pub fn recent(created: u64, now: u64) -> Result<(), Error> {
    if now < created || now - created >= 300000 {
        Err(Error::RecentAuthentication)
    } else {
        Ok(())
    }
}
pub fn revision(current: u64, expected: u64) -> Result<u64, Error> {
    crate::registration::next_revision(current, expected).map_err(|_| Error::Conflict)
}
#[cfg(test)]
#[path = "../tests/unit/profiles.rs"]
mod tests;
