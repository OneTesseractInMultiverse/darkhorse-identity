//! A public transaction selector is usable only with its matching HttpOnly proof.
use super::*;
use std::collections::BTreeMap;
pub(super) const PREFIX: &str = "__Host-darkhorse-authorization-";
pub(super) fn name(reference: &str) -> String {
    format!("{PREFIX}{reference}")
}
pub(super) fn reference(query: Option<&str>) -> Result<&str, Error> {
    let reference = query
        .and_then(|q| q.strip_prefix("request="))
        .ok_or(Error::InvalidTransaction)?;
    session_secret::decode(reference).map_err(|_| Error::InvalidTransaction)?;
    Ok(reference)
}
pub(super) fn select(headers: &HeaderMap, reference: &str) -> Result<[u8; 32], Error> {
    let expected = session_secret::decode(reference).map_err(|_| Error::InvalidTransaction)?;
    let proofs = proofs(headers)?;
    match proofs.get(reference) {
        Some(digest) if digest == &expected => Ok(expected),
        _ => Err(Error::InvalidTransaction),
    }
}
pub(super) fn admit(headers: &HeaderMap) -> Result<(), Error> {
    if proofs(headers)?.len() >= 8 {
        return Err(Error::Unavailable);
    }
    Ok(())
}
fn proofs(headers: &HeaderMap) -> Result<BTreeMap<&str, [u8; 32]>, Error> {
    let mut size = 0usize;
    let mut proofs = BTreeMap::new();
    for value in headers.get_all(header::COOKIE) {
        size = size
            .checked_add(value.as_bytes().len())
            .ok_or(Error::InvalidTransaction)?;
        if size > 4096 {
            return Err(Error::InvalidTransaction);
        }
        let value = value.to_str().map_err(|_| Error::InvalidTransaction)?;
        for part in value.split(';') {
            let (name, secret) = part
                .trim()
                .split_once('=')
                .ok_or(Error::InvalidTransaction)?;
            let Some(reference) = name.strip_prefix(PREFIX) else {
                continue;
            };
            let expected =
                session_secret::decode(reference).map_err(|_| Error::InvalidTransaction)?;
            let digest = handle_digest(secret).map_err(|_| Error::InvalidTransaction)?;
            if digest != expected || proofs.insert(reference, digest).is_some() {
                return Err(Error::InvalidTransaction);
            }
        }
    }
    Ok(proofs)
}
pub(super) fn cookie(reference: &str, secret: &str, seconds: u16) -> Result<HeaderValue, Error> {
    let expected = session_secret::decode(reference).map_err(|_| Error::InvalidTransaction)?;
    match seconds {
        0 if secret.is_empty() => {}
        300 if handle_digest(secret).ok() == Some(expected) => {}
        _ => return Err(Error::InvalidTransaction),
    }
    HeaderValue::from_str(&format!(
        "{}={secret}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age={seconds}",
        name(reference)
    ))
    .map_err(|_| Error::Unavailable)
}
pub(super) fn clear(mut response: Response, reference: &str) -> Response {
    match cookie(reference, "", 0) {
        Ok(cookie) => {
            response.headers_mut().append(header::SET_COOKIE, cookie);
            response
        }
        Err(error) => response::error(error),
    }
}
#[cfg(test)]
#[path = "../../tests/unit/provider_http/context.rs"]
mod tests;
