use super::*;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    revision: String,
    first_name: String,
    second_name: String,
    last_name: String,
    second_last_name: String,
    country: String,
    calling_code: String,
    national_number: String,
    bio: String,
}
pub(super) fn prepare(input: Input) -> Result<(u64, Fields), Error> {
    let revision =
        crate::admin_directory_http::counter(&input.revision).map_err(|_| Error::Invalid)?;
    let phone = if input.calling_code.is_empty() && input.national_number.is_empty() {
        None
    } else {
        Some(Phone::new(&input.calling_code, &input.national_number)?)
    };
    let fields = crate::profiles::prepare(darkhorse_domain::profiles::Input {
        first_name: input.first_name,
        second_name: input.second_name,
        last_name: input.last_name,
        second_last_name: input.second_last_name,
        country: input.country,
        bio: input.bio,
        phone,
    })?;
    Ok((revision, fields))
}
pub(crate) fn target(value: &str) -> Result<Option<PrincipalId>, Error> {
    if value == "me" {
        return Ok(None);
    }
    let id = uuid::Uuid::parse_str(value).map_err(|_| Error::Invalid)?;
    if id.to_string() != value {
        return Err(Error::Invalid);
    }
    PrincipalId::from_u128(id.as_u128())
        .map(Some)
        .map_err(|_| Error::Invalid)
}
#[cfg(test)]
#[path = "../../tests/unit/profiles_http/input.rs"]
mod tests;
