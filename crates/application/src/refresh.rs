//! Atomic, authenticated refresh operations; no raw credentials are persisted.
use crate::tokens::{Material, Tokens};
use darkhorse_domain::{identity::ClientId, tokens::Error};
use std::future::Future;

pub struct Request {
    pub client: ClientId,
    pub secret: [u8; 32],
    pub digest: [u8; 32],
    pub scopes: Option<Vec<String>>,
    pub resource: Option<String>,
}
pub trait RefreshStore: Send + Sync {
    fn refresh(
        &self,
        request: Request,
        material: Material,
        issuer: &str,
    ) -> impl Future<Output = Result<Tokens, Error>> + Send;
}
pub trait RefreshMaintenance: Send + Sync {
    /// Remove one bounded batch of expired families after the retention window.
    fn prune_refresh(&self) -> impl Future<Output = Result<u64, Error>> + Send;
}
/// A bounded maintenance pass; an incomplete batch ends the pass early.
pub async fn sweep(store: &impl RefreshMaintenance) -> Result<(), Error> {
    for _ in 0..10 {
        if store.prune_refresh().await? < u64::from(darkhorse_domain::refresh::CLEANUP_BATCH) {
            return Ok(());
        }
    }
    Ok(())
}
#[cfg(test)]
#[path = "../tests/unit/refresh.rs"]
mod tests;
