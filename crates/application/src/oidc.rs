//! Atomic pending requests. Implementations recheck current primary authority on every call.
use darkhorse_domain::oidc::{Error, Interaction, Request};
use std::future::Future;
pub struct View {
    pub client_name: String,
    pub scopes: Vec<String>,
    pub resource: Option<String>,
    pub interaction: Interaction,
}
pub struct ReturnTo {
    pub uri: String,
    pub state: Option<String>,
}
pub enum Outcome {
    Pending(View),
    Return { target: ReturnTo, error: Error },
}
pub use darkhorse_domain::oidc::Decision;
pub trait AuthorizationStore: Send + Sync {
    fn begin(
        &self,
        request: Request,
        handle: [u8; 32],
        session: Option<[u8; 32]>,
    ) -> impl Future<Output = Result<Outcome, Error>> + Send;
    fn resume(
        &self,
        handle: [u8; 32],
        session: Option<[u8; 32]>,
        decision: Decision,
    ) -> impl Future<Output = Result<Outcome, Error>> + Send;
}
