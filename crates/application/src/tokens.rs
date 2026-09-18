use crate::{oidc::ReturnTo, signing::WrappedKey};
use darkhorse_domain::{
    identity::{ClientId, PrincipalId},
    tokens::Error,
};
use std::future::Future;
// Credentials and claim-bearing objects intentionally have no Debug implementation.
pub struct Opaque {
    pub value: String,
    pub digest: [u8; 32],
}
pub struct Code {
    pub target: ReturnTo,
    pub value: String,
}
pub struct Redemption {
    pub client: ClientId,
    pub secret: [u8; 32],
    pub code: [u8; 32],
    pub redirect: String,
    pub challenge: [u8; 32],
}
pub struct IdClaims {
    pub issuer: String,
    pub client: ClientId,
    pub subject: PrincipalId,
    pub nonce: Option<String>,
    pub authenticated: u64,
    pub issued: u64,
    pub expires: u64,
}
pub struct Tokens {
    pub access: String,
    pub id_token: String,
    pub expires_in: u64,
}
pub trait IdSigner: Send + Sync {
    fn sign(
        &self,
        key: WrappedKey,
        claims: IdClaims,
    ) -> impl Future<Output = Result<String, Error>> + Send;
}
pub trait CodeStore: Send + Sync {
    fn issue(
        &self,
        handle: [u8; 32],
        session: Option<[u8; 32]>,
        code: Opaque,
    ) -> impl Future<Output = Result<Code, Error>> + Send;
}
pub trait TokenStore: Send + Sync {
    fn redeem<S: IdSigner>(
        &self,
        input: Redemption,
        access: Opaque,
        issuer: &str,
        signer: &S,
    ) -> impl Future<Output = Result<Tokens, Error>> + Send;
    fn userinfo(
        &self,
        digest: [u8; 32],
        issuer: &str,
    ) -> impl Future<Output = Result<PrincipalId, Error>> + Send;
}
