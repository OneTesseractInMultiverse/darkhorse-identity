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
pub struct Material {
    pub access: Opaque,
    pub refresh: Opaque,
}
pub struct Code {
    pub target: ReturnTo,
    pub value: String,
}
pub struct Redemption {
    pub resource: Option<String>,
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
    pub refresh: Option<String>,
    pub id_token: Option<String>,
    pub expires_in: u64,
    pub scope: String,
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
        material: Material,
        issuer: &str,
        signer: &S,
    ) -> impl Future<Output = Result<Tokens, Error>> + Send;
    fn userinfo(
        &self,
        digest: [u8; 32],
        issuer: &str,
    ) -> impl Future<Output = Result<UserInfo, Error>> + Send;
}

// Only approved profile fields cross this port. No credential material is projected.
pub struct UserInfo {
    pub subject: PrincipalId,
    pub profile: Option<Names>,
    pub email: Option<String>,
    pub email_verified: bool,
}
pub struct Names {
    pub given: String,
    pub family: String,
}
pub struct Management {
    pub client: ClientId,
    pub secret: [u8; 32],
    pub token: Option<ManagedToken>,
}
pub enum ManagedToken {
    Access([u8; 32]),
    Refresh([u8; 32]),
}
pub struct ActiveToken {
    pub subject: PrincipalId,
    pub client: ClientId,
    pub scope: String,
    pub issued: u64,
    pub expires: u64,
}
pub trait TokenManagementStore: Send + Sync {
    fn introspect(
        &self,
        input: Management,
        issuer: &str,
    ) -> impl Future<Output = Result<Option<ActiveToken>, Error>> + Send;
    fn revoke(
        &self,
        input: Management,
        issuer: &str,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
