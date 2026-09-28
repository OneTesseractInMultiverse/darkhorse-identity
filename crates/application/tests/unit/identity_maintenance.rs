use super::*;
use std::{
    collections::VecDeque,
    future::Future,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

struct Fake(
    Mutex<
        VecDeque<(
            CredentialRecordCategory,
            Result<CredentialRecordCleanupBatch, Error>,
        )>,
    >,
);
impl CredentialRecordMaintenance for Fake {
    async fn prune_expired_credential_records(
        &self,
        category: CredentialRecordCategory,
    ) -> Result<CredentialRecordCleanupBatch, Error> {
        let (expected, result) = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected extra cleanup batch");
        assert_eq!(category, expected);
        result
    }
}

fn run(
    future: impl Future<Output = Result<CredentialRecordCleanupSweep, Error>>,
) -> Result<CredentialRecordCleanupSweep, Error> {
    match std::pin::pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("in-memory cleanup fake must be immediately ready"),
    }
}

fn queued(
    category: CredentialRecordCategory,
    deleted: u64,
    age: Option<u64>,
    backlog: bool,
) -> (
    CredentialRecordCategory,
    Result<CredentialRecordCleanupBatch, Error>,
) {
    (
        category,
        Ok(CredentialRecordCleanupBatch {
            deleted,
            oldest_expired_age_ms: age,
            backlog_remaining: backlog,
        }),
    )
}

#[test]
fn access_token_sweep_aggregates_bounded_batches_and_stops_when_drained() {
    let category = CredentialRecordCategory::LegacyAccessTokens;
    let fake = Fake(Mutex::new(VecDeque::from([
        queued(category, 100, Some(400), true),
        queued(category, 12, Some(900), false),
    ])));
    assert_eq!(
        run(sweep_expired_legacy_access_tokens(&fake)),
        Ok(CredentialRecordCleanupSweep {
            batches: 2,
            deleted: 112,
            oldest_expired_age_ms: Some(900),
            backlog_remaining: false,
        })
    );
    assert!(fake.0.lock().unwrap().is_empty());
}

#[test]
fn authorization_code_sweep_is_bounded_and_reports_deferred_backlog() {
    let category = CredentialRecordCategory::AuthorizationCodes;
    let fake = Fake(Mutex::new(VecDeque::from(
        [queued(category, 100, Some(1_200), true); CREDENTIAL_RECORD_CLEANUP_MAX_BATCHES as usize],
    )));
    assert_eq!(
        run(sweep_expired_authorization_codes(&fake)),
        Ok(CredentialRecordCleanupSweep {
            batches: CREDENTIAL_RECORD_CLEANUP_MAX_BATCHES,
            deleted: 1_000,
            oldest_expired_age_ms: Some(1_200),
            backlog_remaining: true,
        })
    );
    assert!(fake.0.lock().unwrap().is_empty());
}

#[test]
fn a_failed_category_batch_returns_no_partial_success_and_stops() {
    let category = CredentialRecordCategory::LegacyAccessTokens;
    let fake = Fake(Mutex::new(VecDeque::from([
        queued(category, 100, Some(400), true),
        (category, Err(Error::Unavailable)),
        queued(category, 0, None, false),
    ])));
    assert_eq!(
        run(sweep_expired_legacy_access_tokens(&fake)),
        Err(Error::Unavailable)
    );
    assert_eq!(fake.0.lock().unwrap().len(), 1);
}

#[test]
fn cleanup_batch_projection_is_checked_and_age_uses_the_oldest_eligible_row() {
    assert_eq!(
        record_credential_cleanup_batch(
            CredentialRecordCleanupSweep {
                batches: 1,
                deleted: u64::MAX,
                oldest_expired_age_ms: None,
                backlog_remaining: false,
            },
            CredentialRecordCleanupBatch {
                deleted: 1,
                oldest_expired_age_ms: Some(42),
                backlog_remaining: true,
            },
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(
        record_credential_cleanup_batch(
            CredentialRecordCleanupSweep {
                batches: 0,
                deleted: 0,
                oldest_expired_age_ms: None,
                backlog_remaining: false,
            },
            CredentialRecordCleanupBatch {
                deleted: 2,
                oldest_expired_age_ms: Some(42),
                backlog_remaining: false,
            },
        ),
        Ok(CredentialRecordCleanupSweep {
            batches: 1,
            deleted: 2,
            oldest_expired_age_ms: Some(42),
            backlog_remaining: false,
        })
    );
}
