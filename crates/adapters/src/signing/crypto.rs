use super::configuration::WrapKey;
use aws_lc_rs::{
    aead::{self, Aad, LessSafeKey, Nonce, UnboundKey},
    encoding::{AsDer, Pkcs8V1Der},
    rsa::{KeyPair, KeySize},
    signature::KeyPair as _,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use darkhorse_application::signing::{PublicKey, WrappedKey};
use darkhorse_domain::signing::KeyError;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

pub fn generate(issuer: &str, wrap: &WrapKey) -> Result<WrappedKey, KeyError> {
    let key = KeyPair::generate(KeySize::Rsa3072).map_err(unavailable)?;
    prepare(issuer, wrap, &key)
}
pub fn import(issuer: &str, wrap: &WrapKey, der: &[u8]) -> Result<WrappedKey, KeyError> {
    let key = parse(der)?;
    prepare(issuer, wrap, &key)
}
fn prepare(issuer: &str, wrap: &WrapKey, key: &KeyPair) -> Result<WrappedKey, KeyError> {
    let mut nonce = [0; 12];
    getrandom::fill(&mut nonce).map_err(unavailable)?;
    wrap_key(issuer, wrap, key, nonce)
}
fn parse(der: &[u8]) -> Result<KeyPair, KeyError> {
    if der.is_empty() || der.len() > 8192 {
        return Err(KeyError::Invalid);
    }
    let key = KeyPair::from_pkcs8(der).map_err(|_| KeyError::Invalid)?;
    let public = key.public_key();
    if key.public_modulus_len() != 384
        || public
            .modulus()
            .big_endian_without_leading_zero()
            .first()
            .is_none_or(|byte| byte & 0x80 == 0)
        || public.exponent().big_endian_without_leading_zero() != [1, 0, 1]
    {
        return Err(KeyError::Invalid);
    }
    Ok(key)
}
pub fn public(key: &KeyPair) -> PublicKey {
    let public = key.public_key();
    let n = URL_SAFE_NO_PAD.encode(public.modulus().big_endian_without_leading_zero());
    let e = URL_SAFE_NO_PAD.encode(public.exponent().big_endian_without_leading_zero());
    let canonical = format!(r#"{{"e":"{e}","kty":"RSA","n":"{n}"}}"#);
    let kid = URL_SAFE_NO_PAD.encode(Sha256::digest(canonical.as_bytes()));
    PublicKey { kid, n, e }
}
fn wrap_key(
    issuer: &str,
    wrap: &WrapKey,
    key: &KeyPair,
    nonce: [u8; 12],
) -> Result<WrappedKey, KeyError> {
    let public = public(key);
    let document = AsDer::<Pkcs8V1Der>::as_der(key).map_err(unavailable)?;
    let mut plaintext = Zeroizing::new(document.as_ref().to_vec());
    encryption_key(wrap)?
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad(issuer, &public.kid)),
            &mut *plaintext,
        )
        .map_err(unavailable)?;
    Ok(WrappedKey {
        public,
        nonce,
        ciphertext: plaintext.to_vec(),
    })
}
pub fn unwrap(issuer: &str, wrap: &WrapKey, key: &WrappedKey) -> Result<KeyPair, KeyError> {
    let mut bytes = Zeroizing::new(key.ciphertext.clone());
    let plaintext = encryption_key(wrap)?
        .open_in_place(
            Nonce::assume_unique_for_key(key.nonce),
            Aad::from(aad(issuer, &key.public.kid)),
            &mut bytes,
        )
        .map_err(unavailable)?;
    let parsed = parse(plaintext)?;
    if public(&parsed) != key.public {
        return Err(KeyError::Invalid);
    }
    Ok(parsed)
}
fn encryption_key(wrap: &WrapKey) -> Result<LessSafeKey, KeyError> {
    UnboundKey::new(&aead::AES_256_GCM, wrap.bytes())
        .map(LessSafeKey::new)
        .map_err(unavailable)
}
fn aad(issuer: &str, kid: &str) -> Vec<u8> {
    format!("darkhorse:signing-key:v1\0{issuer}\0{kid}").into_bytes()
}
fn unavailable<T>(_: T) -> KeyError {
    KeyError::Unavailable
}
#[cfg(test)]
#[path = "../../tests/unit/signing/crypto.rs"]
mod tests;
