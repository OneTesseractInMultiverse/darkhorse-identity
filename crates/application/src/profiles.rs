//! Project-owned profile ports; the store rechecks current authority on every operation.
use darkhorse_domain::{
    identity::PrincipalId,
    localization::Locale,
    profiles::{Error, Fields},
};
use std::future::Future;
#[derive(Debug, Clone)]
pub struct Profile {
    pub id: PrincipalId,
    pub email: String,
    pub active: bool,
    pub email_verified: bool,
    pub revision: u64,
    pub fields: Fields,
    pub locale: Option<Locale>,
}
pub trait Store: Send + Sync {
    fn update_language(
        &self,
        actor: [u8; 32],
        expected: u64,
        locale: Option<Locale>,
    ) -> impl Future<Output = Result<Profile, Error>> + Send;
    fn profile(
        &self,
        actor: [u8; 32],
        target: Option<PrincipalId>,
    ) -> impl Future<Output = Result<Profile, Error>> + Send;
    fn update_profile(
        &self,
        actor: [u8; 32],
        target: Option<PrincipalId>,
        expected: u64,
        fields: Fields,
    ) -> impl Future<Output = Result<Profile, Error>> + Send;
}
