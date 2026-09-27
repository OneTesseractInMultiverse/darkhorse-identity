use super::*;
use std::{
    collections::VecDeque,
    future::Future,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

struct Fake(Mutex<VecDeque<Result<AuthorizationRequestCleanupBatch, Error>>>);
impl AuthorizationMaintenance for Fake {
    async fn prune_expired_authorization_requests(
        &self,
    ) -> Result<AuthorizationRequestCleanupBatch, Error> {
        self.0
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected extra cleanup batch")
    }
}
fn run(
    future: impl Future<Output = Result<AuthorizationRequestCleanupSweep, Error>>,
) -> Result<AuthorizationRequestCleanupSweep, Error> {
    match std::pin::pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("in-memory cleanup fake must be immediately ready"),
    }
}
fn batch(
    deleted: u64,
    oldest_expired_age_ms: Option<u64>,
    backlog_remaining: bool,
) -> Result<AuthorizationRequestCleanupBatch, Error> {
    Ok(AuthorizationRequestCleanupBatch {
        deleted,
        oldest_expired_age_ms,
        backlog_remaining,
    })
}

#[test]
fn short_batches_report_deletions_and_drain_without_claiming_a_backlog() {
    for deleted in [0, 1, u64::from(AUTHORIZATION_REQUEST_CLEANUP_BATCH) - 1] {
        let fake = Fake(Mutex::new(VecDeque::from([batch(
            deleted,
            (deleted > 0).then_some(600_000),
            false,
        )])));
        assert_eq!(
            run(sweep_expired_authorization_requests(&fake)),
            Ok(AuthorizationRequestCleanupSweep {
                batches: 1,
                deleted,
                oldest_expired_age_ms: (deleted > 0).then_some(600_000),
                backlog_remaining: false,
            })
        );
        assert!(fake.0.lock().unwrap().is_empty());
    }
}

#[test]
fn full_batches_continue_and_aggregate_oldest_age_and_backlog_state() {
    let fake = Fake(Mutex::new(VecDeque::from([
        batch(100, Some(600_000), true),
        batch(100, Some(900_000), true),
        batch(100, Some(1_200_000), false),
        batch(0, None, false),
    ])));
    assert_eq!(
        run(sweep_expired_authorization_requests(&fake)),
        Ok(AuthorizationRequestCleanupSweep {
            batches: 4,
            deleted: 300,
            oldest_expired_age_ms: Some(1_200_000),
            backlog_remaining: false,
        })
    );
    assert!(fake.0.lock().unwrap().is_empty());
}

#[test]
fn full_sweep_stops_at_its_batch_limit_and_reports_remaining_work() {
    let fake = Fake(Mutex::new(VecDeque::from(
        [batch(100, Some(600_000), true); AUTHORIZATION_REQUEST_CLEANUP_MAX_BATCHES as usize],
    )));
    assert_eq!(
        run(sweep_expired_authorization_requests(&fake)),
        Ok(AuthorizationRequestCleanupSweep {
            batches: AUTHORIZATION_REQUEST_CLEANUP_MAX_BATCHES,
            deleted: 1_000,
            oldest_expired_age_ms: Some(600_000),
            backlog_remaining: true,
        })
    );
    assert!(fake.0.lock().unwrap().is_empty());
}

#[test]
fn a_failed_batch_returns_no_partial_success_report_and_stops_work() {
    let fake = Fake(Mutex::new(VecDeque::from([
        batch(100, Some(600_000), true),
        Err(Error::Unavailable),
        batch(0, None, false),
    ])));
    assert_eq!(
        run(sweep_expired_authorization_requests(&fake)),
        Err(Error::Unavailable)
    );
    assert_eq!(fake.0.lock().unwrap().len(), 1);
}

#[test]
fn batch_aggregation_is_a_pure_checked_computation() {
    let initial = empty_sweep();
    assert_eq!(
        record_batch(
            initial,
            AuthorizationRequestCleanupBatch {
                deleted: 100,
                oldest_expired_age_ms: Some(300),
                backlog_remaining: true,
            }
        ),
        Ok(AuthorizationRequestCleanupSweep {
            batches: 1,
            deleted: 100,
            oldest_expired_age_ms: Some(300),
            backlog_remaining: true,
        })
    );
    assert!(batch_fills_limit(AuthorizationRequestCleanupBatch {
        deleted: 100,
        oldest_expired_age_ms: None,
        backlog_remaining: false,
    }));
    assert!(!batch_fills_limit(AuthorizationRequestCleanupBatch {
        deleted: 99,
        oldest_expired_age_ms: None,
        backlog_remaining: false,
    }));
    assert_eq!(
        record_batch(
            AuthorizationRequestCleanupSweep {
                batches: 1,
                deleted: u64::MAX,
                oldest_expired_age_ms: None,
                backlog_remaining: false,
            },
            AuthorizationRequestCleanupBatch {
                deleted: 1,
                oldest_expired_age_ms: None,
                backlog_remaining: false,
            }
        ),
        Err(Error::Unavailable)
    );
}
