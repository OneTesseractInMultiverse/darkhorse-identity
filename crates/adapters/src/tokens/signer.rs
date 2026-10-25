use crate::signing::{configuration::WrapKey, crypto};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use darkhorse_application::{
    logout::{LogoutClaims, LogoutSigner},
    signing::WrappedKey,
    tokens::{IdClaims, IdSigner},
};
use darkhorse_domain::{
    signing::{Purpose, logout_expiration, message_times},
    tokens::Error,
};
use std::sync::Arc;
use tokio::sync::Semaphore;
#[derive(Clone)]
pub struct Signer {
    wrap: Arc<WrapKey>,
    slots: Arc<Semaphore>,
}
enum Claims {
    Identity(IdClaims),
    Logout(LogoutClaims),
}
impl Claims {
    fn issuer(&self) -> &str {
        match self {
            Self::Identity(claims) => &claims.issuer,
            Self::Logout(claims) => &claims.issuer,
        }
    }
    fn message(&self, kid: &str) -> Result<String, Error> {
        match self {
            Self::Identity(claims) => message(kid, claims),
            Self::Logout(claims) => logout_message(kid, claims),
        }
    }
}
impl Signer {
    pub fn new(wrap: WrapKey) -> Self {
        Self {
            wrap: Arc::new(wrap),
            slots: Arc::new(Semaphore::new(4)),
        }
    }
    async fn sign_claims(&self, key: WrappedKey, claims: Claims) -> Result<String, Error> {
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
impl IdSigner for Signer {
    async fn sign(&self, key: WrappedKey, claims: IdClaims) -> Result<String, Error> {
        self.sign_claims(key, Claims::Identity(claims)).await
    }
}
impl LogoutSigner for Signer {
    async fn sign_logout(&self, key: WrappedKey, claims: LogoutClaims) -> Result<String, Error> {
        self.sign_claims(key, Claims::Logout(claims)).await
    }
}
fn sign(wrap: &WrapKey, key: WrappedKey, claims: Claims) -> Result<String, Error> {
    let message = claims.message(&key.public.kid)?;
    let pair = crypto::unwrap(claims.issuer(), wrap, &key).map_err(unavailable)?;
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
    let mut claims = serde_json::json!({"iss":c.issuer,"aud":uuid::Uuid::from_u128(c.client.as_u128()).to_string(),"sub":uuid::Uuid::from_u128(c.subject.as_u128()).to_string(),"sid":uuid::Uuid::from_u128(c.session.as_u128()).to_string(),"iat":c.issued,"exp":c.expires,"auth_time":c.authenticated});
    if let Some(nonce) = &c.nonce {
        claims["nonce"] = nonce.clone().into();
    }
    Ok(encode(Purpose::IdToken, kid, claims))
}
fn logout_message(kid: &str, c: &LogoutClaims) -> Result<String, Error> {
    let expires = logout_expiration(c.issued).map_err(unavailable)?;
    let claims = serde_json::json!({
        "iss": c.issuer,
        "aud": uuid::Uuid::from_u128(c.client.as_u128()).to_string(),
        "sub": uuid::Uuid::from_u128(c.subject.as_u128()).to_string(),
        "sid": uuid::Uuid::from_u128(c.session.as_u128()).to_string(),
        "jti": uuid::Uuid::from_u128(c.token.as_u128()).to_string(),
        "iat": c.issued, "exp": expires,
        "events": {"http://schemas.openid.net/event/backchannel-logout": {}}
    });
    Ok(encode(Purpose::LogoutToken, kid, claims))
}
fn encode(purpose: Purpose, kid: &str, claims: serde_json::Value) -> String {
    let header = serde_json::json!({"alg":"RS256","typ":purpose.header_type(),"kid":kid});
    format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header.to_string()),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    )
}
fn unavailable<T>(_: T) -> Error {
    Error::Unavailable
}

#[cfg(test)]
#[path = "../../tests/unit/tokens/signer.rs"]
mod tests;
