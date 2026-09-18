use crate::{
    provider_http::request::decode,
    tokens::material::{self, Purpose},
};
use axum::http::HeaderMap;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use darkhorse_application::tokens::Redemption;
use darkhorse_domain::{identity::ClientId, tokens::Error};
use std::collections::BTreeMap;
use zeroize::Zeroizing;
fn one<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, Error> {
    let mut values = headers.get_all(name).iter();
    let value = values
        .next()
        .ok_or(Error::InvalidClient)?
        .to_str()
        .map_err(|_| Error::InvalidClient)?;
    if values.next().is_some() {
        return Err(Error::InvalidClient);
    }
    Ok(value)
}
pub(super) fn basic(headers: &HeaderMap) -> Result<(ClientId, [u8; 32]), Error> {
    let value = one(headers, "authorization")?;
    if value.len() > 2048 {
        return Err(Error::InvalidClient);
    }
    let (scheme, data) = value.split_once(' ').ok_or(Error::InvalidClient)?;
    if !scheme.eq_ignore_ascii_case("Basic") {
        return Err(Error::InvalidClient);
    }
    let decoded = Zeroizing::new(STANDARD.decode(data).map_err(|_| Error::InvalidClient)?);
    let (id, secret) = std::str::from_utf8(&decoded)
        .map_err(|_| Error::InvalidClient)?
        .split_once(':')
        .ok_or(Error::InvalidClient)?;
    let id = decode(id).map_err(|_| Error::InvalidClient)?;
    let secret = Zeroizing::new(decode(secret).map_err(|_| Error::InvalidClient)?);
    let uuid = uuid::Uuid::parse_str(&id).map_err(|_| Error::InvalidClient)?;
    if uuid.to_string() != id {
        return Err(Error::InvalidClient);
    }
    Ok((
        ClientId::from_u128(uuid.as_u128()).map_err(|_| Error::InvalidClient)?,
        crate::registration::secret_digest(&secret).map_err(|_| Error::InvalidClient)?,
    ))
}
pub(super) fn bearer(headers: &HeaderMap) -> Result<[u8; 32], Error> {
    let (scheme, value) = one(headers, "authorization")
        .map_err(|_| Error::InvalidToken)?
        .split_once(' ')
        .ok_or(Error::InvalidToken)?;
    if !scheme.eq_ignore_ascii_case("Bearer") {
        return Err(Error::InvalidToken);
    }
    material::digest(value, Purpose::Access).map_err(|_| Error::InvalidToken)
}
pub(super) fn request(headers: &HeaderMap, body: &[u8]) -> Result<Redemption, Error> {
    if one(headers, "content-type")
        .map_err(|_| Error::InvalidRequest)?
        .split(';')
        .next()
        != Some("application/x-www-form-urlencoded")
    {
        return Err(Error::InvalidRequest);
    }
    let (client, secret) = basic(headers)?;
    let values = form(std::str::from_utf8(body).map_err(|_| Error::InvalidRequest)?)?;
    let get = |key: &str| {
        values
            .get(key)
            .map(String::as_str)
            .ok_or(Error::InvalidRequest)
    };
    Ok(Redemption {
        client,
        secret,
        code: material::digest(get("code")?, Purpose::Code)?,
        redirect: get("redirect_uri")?.into(),
        challenge: material::challenge(get("code_verifier")?)?,
    })
}
fn form(value: &str) -> Result<BTreeMap<String, String>, Error> {
    if value.len() > 4096 {
        return Err(Error::InvalidRequest);
    }
    let mut fields = BTreeMap::new();
    for (index, pair) in value.split('&').enumerate() {
        if index >= 16 {
            return Err(Error::InvalidRequest);
        }
        let (key, value) = pair.split_once('=').ok_or(Error::InvalidRequest)?;
        let key = decode(key).map_err(|_| Error::InvalidRequest)?;
        let value = decode(value).map_err(|_| Error::InvalidRequest)?;
        if [
            "client_id",
            "client_secret",
            "client_assertion",
            "client_assertion_type",
        ]
        .contains(&key.as_str())
        {
            return Err(Error::InvalidRequest);
        }
        if fields.insert(key, value).is_some() {
            return Err(Error::InvalidRequest);
        }
    }
    match fields.get("grant_type").map(String::as_str) {
        Some("authorization_code") => Ok(fields),
        Some(_) => Err(Error::UnsupportedGrant),
        None => Err(Error::InvalidRequest),
    }
}
#[cfg(test)]
#[path = "../../tests/unit/token_http/input.rs"]
mod tests;
