//! Bounded lifecycle work for short-lived authorization transactions.
use darkhorse_domain::oidc::Error;
use std::future::Future;

pub const AUTHORIZATION_REQUEST_CLEANUP_BATCH: u32 = 100;
pub const AUTHORIZATION_REQUEST_CLEANUP_MAX_BATCHES: u32 = 10;

pub trait AuthorizationMaintenance: Send + Sync {
    /// Remove one batch of expired requests and return its row count.
    fn prune_expired_authorization_requests(
        &self,
    ) -> impl Future<Output = Result<u64, Error>> + Send;
}

/// Drain at most ten batches so maintenance yields back to the server promptly.
pub async fn sweep_expired_authorization_requests(
    store: &impl AuthorizationMaintenance,
) -> Result<(), Error> {
    for _ in 0..AUTHORIZATION_REQUEST_CLEANUP_MAX_BATCHES {
        if store.prune_expired_authorization_requests().await?
            < u64::from(AUTHORIZATION_REQUEST_CLEANUP_BATCH)
        {
            return Ok(());
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/oidc_maintenance.rs"]
mod tests;
