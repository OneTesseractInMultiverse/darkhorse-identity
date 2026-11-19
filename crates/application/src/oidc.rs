//! Atomic pending requests. Implementations recheck current primary authority on every call.
use darkhorse_domain::oidc::{Error, Interaction, Request};
use std::future::Future;
pub struct View {
    pub client_name: String,
    pub scopes: Vec<String>,
    pub resource: Option<String>,
    pub interaction: Interaction,
    pub ui_locale: Option<darkhorse_domain::localization::Locale>,
}
pub struct ReturnTo {
    pub uri: String,
    pub state: Option<String>,
}
pub enum Outcome {
    Pending(View),
    Login(Box<Preauthorization>),
    Return { target: ReturnTo, error: Error },
}
/// Immutable pre-login facts, authenticated by the transport before continuation.
/// These facts are not authority: the store rechecks current primary state.
#[derive(Clone)]
pub struct Preauthorization {
    pub request: Request,
    pub created_ms: u64,
    pub initial_session: Option<[u8; 32]>,
    pub client_revision: u64,
    pub application_revision: u64,
    pub resource_policy_revision: Option<u64>,
}
pub use darkhorse_domain::oidc::Decision;
pub trait AuthorizationStore: Send + Sync {
    fn begin(
        &self,
        request: Request,
        handle: [u8; 32],
        session: Option<[u8; 32]>,
    ) -> impl Future<Output = Result<Outcome, Error>> + Send;
    fn continue_login(
        &self,
        pending: Preauthorization,
        handle: [u8; 32],
        session: Option<[u8; 32]>,
        decision: Decision,
    ) -> impl Future<Output = Result<Outcome, Error>> + Send;
    fn resume(
        &self,
        handle: [u8; 32],
        session: Option<[u8; 32]>,
        decision: Decision,
    ) -> impl Future<Output = Result<Outcome, Error>> + Send;
}
