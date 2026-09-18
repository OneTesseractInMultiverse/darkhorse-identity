use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use darkhorse_domain::{
    identity::ClientId,
    oidc::{Error, Prompt, Request},
};
use std::collections::BTreeMap;
const PARAMETERS: &[&str] = &[
    "client_id",
    "redirect_uri",
    "response_type",
    "response_mode",
    "scope",
    "resource",
    "code_challenge",
    "code_challenge_method",
    "state",
    "nonce",
    "prompt",
    "max_age",
];
pub(super) fn parse(query: &str) -> Result<Request, Error> {
    let fields = fields(query)?;
    if required(&fields, "response_type")? != "code"
        || fields
            .get("response_mode")
            .is_some_and(|mode| mode != "query")
        || required(&fields, "code_challenge_method")? != "S256"
    {
        return Err(Error::InvalidRequest);
    }
    let client = required(&fields, "client_id")?;
    let id = uuid::Uuid::parse_str(client).map_err(invalid)?;
    if id.to_string() != client {
        return Err(Error::InvalidRequest);
    }
    let redirect = required(&fields, "redirect_uri")?;
    crate::registration::redirects(vec![redirect.into()]).map_err(invalid)?;
    let url = url::Url::parse(redirect).map_err(invalid)?;
    if url.query_pairs().any(|(key, _)| {
        [
            "code",
            "state",
            "error",
            "error_description",
            "error_uri",
            "iss",
        ]
        .contains(&key.as_ref())
    }) {
        return Err(Error::InvalidRequest);
    }
    let challenge = URL_SAFE_NO_PAD
        .decode(required(&fields, "code_challenge")?)
        .map_err(invalid)?
        .try_into()
        .map_err(invalid)?;
    let request = Request {
        client: ClientId::from_u128(id.as_u128()).map_err(invalid)?,
        redirect: redirect.into(),
        challenge,
        state: fields.get("state").cloned(),
        nonce: fields.get("nonce").cloned(),
        scopes: required(&fields, "scope")?
            .split(' ')
            .map(str::to_owned)
            .collect(),
        resource: fields.get("resource").cloned(),
        prompt: prompt(fields.get("prompt").map(String::as_str))?,
        max_age: fields.get("max_age").map(|value| age(value)).transpose()?,
    };
    request.validate()?;
    Ok(request)
}
fn fields(query: &str) -> Result<BTreeMap<String, String>, Error> {
    if query.is_empty() || query.len() > 8192 {
        return Err(Error::InvalidRequest);
    }
    let mut fields = BTreeMap::new();
    for (index, part) in query.split('&').enumerate() {
        if index >= 32 {
            return Err(Error::InvalidRequest);
        }
        let (key, value) = part.split_once('=').ok_or(Error::InvalidRequest)?;
        let key = decode(key)?;
        let value = decode(value)?;
        if [
            "request",
            "request_uri",
            "claims",
            "id_token_hint",
            "acr_values",
            "login_hint",
        ]
        .contains(&key.as_str())
        {
            return Err(Error::InvalidRequest);
        }
        if PARAMETERS.contains(&key.as_str()) && fields.insert(key, value).is_some() {
            return Err(Error::InvalidRequest);
        }
    }
    Ok(fields)
}
fn decode(value: &str) -> Result<String, Error> {
    let bytes = value.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'%'
            && (index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit())
        {
            return Err(Error::InvalidRequest);
        }
    }
    percent_encoding::percent_decode_str(&value.replace('+', " "))
        .decode_utf8()
        .map(|value| value.into_owned())
        .map_err(invalid)
}
fn required<'a>(fields: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, Error> {
    fields
        .get(key)
        .map(String::as_str)
        .ok_or(Error::InvalidRequest)
}
fn prompt(value: Option<&str>) -> Result<Prompt, Error> {
    match value {
        None => Ok(Prompt::Default),
        Some("none") => Ok(Prompt::None),
        Some("login") => Ok(Prompt::Login),
        Some("consent") => Ok(Prompt::Consent),
        Some("login consent" | "consent login") => Ok(Prompt::LoginConsent),
        _ => Err(Error::InvalidRequest),
    }
}
fn age(value: &str) -> Result<u64, Error> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::InvalidRequest);
    }
    value.parse().map_err(invalid)
}
fn invalid<T>(_: T) -> Error {
    Error::InvalidRequest
}
#[cfg(test)]
#[path = "../../tests/unit/provider_http/request.rs"]
mod tests;
