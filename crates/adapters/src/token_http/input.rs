use crate::{
    provider_http::request::decode,
    tokens::material::{self, Purpose},
};
use axum::http::HeaderMap;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use darkhorse_application::tokens::{Management, Redemption};
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
fn credentials(headers: &HeaderMap) -> Result<(String, Zeroizing<String>), Error> {
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
    Ok((id, secret))
}
pub(super) fn basic(headers: &HeaderMap) -> Result<(ClientId, [u8; 32]), Error> {
    let (id, secret) = credentials(headers)?;
    client_credentials(&id, &secret)
}
fn client_credentials(id: &str, secret: &str) -> Result<(ClientId, [u8; 32]), Error> {
    let uuid = uuid::Uuid::parse_str(id).map_err(|_| Error::InvalidClient)?;
    if uuid.to_string() != id {
        return Err(Error::InvalidClient);
    }
    Ok((
        ClientId::from_u128(uuid.as_u128()).map_err(|_| Error::InvalidClient)?,
        crate::registration::secret_digest(secret).map_err(|_| Error::InvalidClient)?,
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
pub(super) enum Grant {
    Code(Redemption),
    Refresh(darkhorse_application::refresh::Request),
}
pub(super) fn request(headers: &HeaderMap, body: &[u8]) -> Result<Grant, Error> {
    content_type(headers)?;
    let (client, secret) = basic(headers)?;
    let values = form(std::str::from_utf8(body).map_err(|_| Error::InvalidRequest)?)?;
    let get = |key: &str| {
        values
            .get(key)
            .map(String::as_str)
            .ok_or(Error::InvalidRequest)
    };
    if get("grant_type")? == "refresh_token" {
        if ["code", "code_verifier", "redirect_uri"]
            .iter()
            .any(|key| values.contains_key(*key))
        {
            return Err(Error::InvalidRequest);
        }
        return Ok(Grant::Refresh(darkhorse_application::refresh::Request {
            client,
            secret,
            digest: material::digest(get("refresh_token")?, Purpose::Refresh)?,
            scopes: values
                .get("scope")
                .map(|value| value.split(' ').map(String::from).collect()),
            resource: values.get("resource").cloned(),
        }));
    }
    if values.contains_key("refresh_token") {
        return Err(Error::InvalidRequest);
    }
    Ok(Grant::Code(Redemption {
        resource: values.get("resource").cloned(),
        client,
        secret,
        code: material::digest(get("code")?, Purpose::Code)?,
        redirect: get("redirect_uri")?.into(),
        challenge: material::challenge(get("code_verifier")?)?,
    }))
}
fn content_type(headers: &HeaderMap) -> Result<(), Error> {
    if one(headers, "content-type")
        .map_err(|_| Error::InvalidRequest)?
        .split(';')
        .next()
        != Some("application/x-www-form-urlencoded")
    {
        return Err(Error::InvalidRequest);
    }
    Ok(())
}
pub(super) fn management(headers: &HeaderMap, body: &[u8]) -> Result<Management, Error> {
    content_type(headers)?;
    let (client, secret) = basic(headers)?;
    Ok(Management {
        client,
        secret,
        token: managed_token(body)?,
    })
}
fn token(body: &[u8]) -> Result<Option<[u8; 32]>, Error> {
    Ok(material::digest(&token_value(body)?, Purpose::Access).ok())
}
fn managed_token(
    body: &[u8],
) -> Result<Option<darkhorse_application::tokens::ManagedToken>, Error> {
    use darkhorse_application::tokens::ManagedToken;
    let value = token_value(body)?;
    Ok(material::digest(&value, Purpose::Access)
        .map(ManagedToken::Access)
        .or_else(|_| material::digest(&value, Purpose::Refresh).map(ManagedToken::Refresh))
        .ok())
}
fn token_value(body: &[u8]) -> Result<Zeroizing<String>, Error> {
    let values = fields(std::str::from_utf8(body).map_err(|_| Error::InvalidRequest)?)?;
    let value = values
        .get("token")
        .filter(|v| !v.is_empty() && v.len() <= 2048)
        .ok_or(Error::InvalidRequest)?;
    Ok(Zeroizing::new(value.clone()))
}
pub(super) enum Inquiry {
    Client(Management),
    Resource(darkhorse_application::resource_servers::Probe),
    PersonalKey(darkhorse_application::personal_keys::Probe),
}
pub(super) fn introspection(headers: &HeaderMap, body: &[u8]) -> Result<Inquiry, Error> {
    content_type(headers)?;
    let (id, secret) = credentials(headers)?;
    match id.strip_prefix("rs_") {
        Some(id) => {
            let uuid = uuid::Uuid::parse_str(id).map_err(|_| Error::InvalidClient)?;
            if uuid.to_string() != id {
                return Err(Error::InvalidClient);
            }
            let value = token_value(body)?;
            if value.starts_with("dk_") {
                return Ok(Inquiry::PersonalKey(
                    darkhorse_application::personal_keys::Probe {
                        resource: darkhorse_domain::identity::ResourceId::from_u128(uuid.as_u128())
                            .map_err(|_| Error::InvalidClient)?,
                        secret: crate::resource_servers::secret_digest(&secret)
                            .map_err(|_| Error::InvalidClient)?,
                        key: crate::personal_keys::digest(&value).ok(),
                    },
                ));
            }
            Ok(Inquiry::Resource(
                darkhorse_application::resource_servers::Probe {
                    resource: darkhorse_domain::identity::ResourceId::from_u128(uuid.as_u128())
                        .map_err(|_| Error::InvalidClient)?,
                    secret: crate::resource_servers::secret_digest(&secret)
                        .map_err(|_| Error::InvalidClient)?,
                    token: token(body)?,
                },
            ))
        }
        None => {
            let (client, secret) = client_credentials(&id, &secret)?;
            Ok(Inquiry::Client(Management {
                client,
                secret,
                token: managed_token(body)?,
            }))
        }
    }
}

fn form(value: &str) -> Result<BTreeMap<String, String>, Error> {
    let fields = fields(value)?;
    match fields.get("grant_type").map(String::as_str) {
        Some("authorization_code" | "refresh_token") => Ok(fields),
        Some(_) => Err(Error::UnsupportedGrant),
        None => Err(Error::InvalidRequest),
    }
}
fn fields(value: &str) -> Result<BTreeMap<String, String>, Error> {
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
    Ok(fields)
}
#[cfg(test)]
#[path = "../../tests/unit/token_http/input.rs"]
mod tests;
