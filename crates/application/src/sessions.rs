//! Shared session-management boundary for authenticated self-service callers.
use darkhorse_domain::{
    identity::SessionId,
    sessions::{Cursor, Error, Page},
};
use std::future::Future;
#[derive(Debug, PartialEq, Eq)]
pub struct Ended {
    pub current: bool,
}
pub trait SessionManagement: Send + Sync {
    /// Verify the original browser session and return only its owner's records.
    fn sessions(
        &self,
        actor: [u8; 32],
        after: Option<Cursor>,
    ) -> impl Future<Output = Result<Page, Error>> + Send;
    /// Recheck ownership and commit terminal revocation with its audit event.
    fn end_session(
        &self,
        actor: [u8; 32],
        target: SessionId,
    ) -> impl Future<Output = Result<Ended, Error>> + Send;
}
