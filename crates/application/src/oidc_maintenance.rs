//! Bounded lifecycle work for short-lived authorization transactions.
use darkhorse_domain::oidc::Error;
use std::future::Future;

pub const AUTHORIZATION_REQUEST_CLEANUP_BATCH: u32 = 100;
pub const AUTHORIZATION_REQUEST_CLEANUP_MAX_BATCHES: u32 = 10;
pub const CREDENTIAL_RECORD_CLEANUP_BATCH: u32 = 100;
pub const CREDENTIAL_RECORD_CLEANUP_MAX_BATCHES: u32 = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthorizationRequestCleanupBatch {
    pub deleted: u64,
    pub oldest_expired_age_ms: Option<u64>,
    pub backlog_remaining: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthorizationRequestCleanupSweep {
    pub batches: u32,
    pub deleted: u64,
    pub oldest_expired_age_ms: Option<u64>,
    pub backlog_remaining: bool,
}

pub trait AuthorizationMaintenance: Send + Sync {
    /// Remove one batch and return bounded, identifier-free backlog observations.
    fn prune_expired_authorization_requests(
        &self,
    ) -> impl Future<Output = Result<AuthorizationRequestCleanupBatch, Error>> + Send;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialRecordCategory {
    LegacyAccessTokens,
    AuthorizationCodes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CredentialRecordCleanupBatch {
    pub deleted: u64,
    pub oldest_expired_age_ms: Option<u64>,
    pub backlog_remaining: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CredentialRecordCleanupSweep {
    pub batches: u32,
    pub deleted: u64,
    pub oldest_expired_age_ms: Option<u64>,
    pub backlog_remaining: bool,
}

pub trait CredentialRecordMaintenance: Send + Sync {
    /// Delete one bounded, primary-time-eligible batch of the selected record class.
    fn prune_expired_credential_records(
        &self,
        category: CredentialRecordCategory,
    ) -> impl Future<Output = Result<CredentialRecordCleanupBatch, Error>> + Send;
}

pub async fn sweep_expired_legacy_access_tokens(
    store: &impl CredentialRecordMaintenance,
) -> Result<CredentialRecordCleanupSweep, Error> {
    sweep_credential_records(store, CredentialRecordCategory::LegacyAccessTokens).await
}

pub async fn sweep_expired_authorization_codes(
    store: &impl CredentialRecordMaintenance,
) -> Result<CredentialRecordCleanupSweep, Error> {
    sweep_credential_records(store, CredentialRecordCategory::AuthorizationCodes).await
}

async fn sweep_credential_records(
    store: &impl CredentialRecordMaintenance,
    category: CredentialRecordCategory,
) -> Result<CredentialRecordCleanupSweep, Error> {
    let mut sweep = empty_credential_sweep();
    for _ in 0..CREDENTIAL_RECORD_CLEANUP_MAX_BATCHES {
        let batch = store.prune_expired_credential_records(category).await?;
        sweep = record_credential_cleanup_batch(sweep, batch)?;
        if batch.deleted < u64::from(CREDENTIAL_RECORD_CLEANUP_BATCH) {
            return Ok(sweep);
        }
    }
    Ok(sweep)
}

fn empty_credential_sweep() -> CredentialRecordCleanupSweep {
    CredentialRecordCleanupSweep {
        batches: 0,
        deleted: 0,
        oldest_expired_age_ms: None,
        backlog_remaining: false,
    }
}

fn record_credential_cleanup_batch(
    sweep: CredentialRecordCleanupSweep,
    batch: CredentialRecordCleanupBatch,
) -> Result<CredentialRecordCleanupSweep, Error> {
    Ok(CredentialRecordCleanupSweep {
        batches: sweep.batches.checked_add(1).ok_or(Error::Unavailable)?,
        deleted: sweep
            .deleted
            .checked_add(batch.deleted)
            .ok_or(Error::Unavailable)?,
        oldest_expired_age_ms: older(sweep.oldest_expired_age_ms, batch.oldest_expired_age_ms),
        backlog_remaining: batch.backlog_remaining,
    })
}

/// Drain at most ten batches so maintenance yields back to the server promptly.
pub async fn sweep_expired_authorization_requests(
    store: &impl AuthorizationMaintenance,
) -> Result<AuthorizationRequestCleanupSweep, Error> {
    let mut sweep = empty_sweep();
    for _ in 0..AUTHORIZATION_REQUEST_CLEANUP_MAX_BATCHES {
        let batch = store.prune_expired_authorization_requests().await?;
        sweep = record_batch(sweep, batch)?;
        if !batch_fills_limit(batch) {
            return Ok(sweep);
        }
    }
    Ok(sweep)
}

fn empty_sweep() -> AuthorizationRequestCleanupSweep {
    AuthorizationRequestCleanupSweep {
        batches: 0,
        deleted: 0,
        oldest_expired_age_ms: None,
        backlog_remaining: false,
    }
}

fn record_batch(
    sweep: AuthorizationRequestCleanupSweep,
    batch: AuthorizationRequestCleanupBatch,
) -> Result<AuthorizationRequestCleanupSweep, Error> {
    Ok(AuthorizationRequestCleanupSweep {
        batches: sweep.batches.checked_add(1).ok_or(Error::Unavailable)?,
        deleted: sweep
            .deleted
            .checked_add(batch.deleted)
            .ok_or(Error::Unavailable)?,
        oldest_expired_age_ms: older(sweep.oldest_expired_age_ms, batch.oldest_expired_age_ms),
        backlog_remaining: batch.backlog_remaining,
    })
}

fn batch_fills_limit(batch: AuthorizationRequestCleanupBatch) -> bool {
    batch.deleted >= u64::from(AUTHORIZATION_REQUEST_CLEANUP_BATCH)
}

fn older(current: Option<u64>, observed: Option<u64>) -> Option<u64> {
    match (current, observed) {
        (Some(current), Some(observed)) => Some(current.max(observed)),
        (Some(current), None) => Some(current),
        (None, Some(observed)) => Some(observed),
        (None, None) => None,
    }
}

#[cfg(test)]
#[path = "../tests/unit/oidc_maintenance.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/unit/identity_maintenance.rs"]
mod identity_tests;
