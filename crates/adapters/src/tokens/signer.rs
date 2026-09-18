use crate::signing::{configuration::WrapKey, crypto};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use darkhorse_application::{
    signing::WrappedKey,
    tokens::{IdClaims, IdSigner},
};
use darkhorse_domain::{
    signing::{Purpose, message_times},
    tokens::Error,
};
use std::sync::Arc;
use tokio::sync::Semaphore;
pub struct Signer {
    wrap: Arc<WrapKey>,
    slots: Arc<Semaphore>,
}
impl Signer {
    pub fn new(wrap: WrapKey) -> Self {
        Self {
            wrap: Arc::new(wrap),
            slots: Arc::new(Semaphore::new(4)),
        }
    }
}
impl IdSigner for Signer {
    async fn sign(&self, key: WrappedKey, claims: IdClaims) -> Result<String, Error> {
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(unavailable)?;
        let wrap = self.wrap.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            sign(&wrap, key, claims)
        })
        .await
        .map_err(unavailable)?
    }
}
fn sign(wrap: &WrapKey, key: WrappedKey, claims: IdClaims) -> Result<String, Error> {
    let message = message(&key.public.kid, &claims)?;
    let pair = crypto::unwrap(&claims.issuer, wrap, &key).map_err(unavailable)?;
    let mut signature = vec![0; pair.public_modulus_len()];
    pair.sign(
        &aws_lc_rs::signature::RSA_PKCS1_SHA256,
        &aws_lc_rs::rand::SystemRandom::new(),
        message.as_bytes(),
        &mut signature,
    )
    .map_err(unavailable)?;
    Ok(format!("{message}.{}", URL_SAFE_NO_PAD.encode(signature)))
}
fn message(kid: &str, c: &IdClaims) -> Result<String, Error> {
    message_times(c.issued, c.expires).map_err(unavailable)?;
    if c.authenticated > c.issued {
        return Err(Error::Unavailable);
    }
    let header = serde_json::json!({"alg":"RS256","typ":Purpose::IdToken.header_type(),"kid":kid});
    let mut claims = serde_json::json!({"iss":c.issuer,"aud":uuid::Uuid::from_u128(c.client.as_u128()).to_string(),"sub":uuid::Uuid::from_u128(c.subject.as_u128()).to_string(),"iat":c.issued,"exp":c.expires,"auth_time":c.authenticated});
    if let Some(nonce) = &c.nonce {
        claims["nonce"] = nonce.clone().into();
    }
    Ok(format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header.to_string()),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    ))
}
fn unavailable<T>(_: T) -> Error {
    Error::Unavailable
}

#[cfg(test)]
#[path = "../../tests/unit/tokens/signer.rs"]
mod tests;
