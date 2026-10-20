//! Session-specific protocol messages. Signing does not authorize a logout.
use crate::signing::WrappedKey;
use darkhorse_domain::{
    identity::{ClientId, LogoutTokenId, PrincipalId, RelyingPartySessionId},
    tokens::Error,
};
use std::future::Future;

// No Debug: protocol messages contain private session associations.
pub struct LogoutClaims {
    pub issuer: String,
    pub client: ClientId,
    pub subject: PrincipalId,
    pub session: RelyingPartySessionId,
    pub token: LogoutTokenId,
    pub issued: u64,
}

/// Callers must supply a committed logout target, unique token identity, primary
/// time, and a currently authorized signing key. Delivery is a separate effect.
pub trait LogoutSigner: Send + Sync {
    fn sign_logout(
        &self,
        key: WrappedKey,
        claims: LogoutClaims,
    ) -> impl Future<Output = Result<String, Error>> + Send;
}
