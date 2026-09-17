use super::*;
use darkhorse_domain::limiting::{Budget, BudgetRule};
use std::{
    future::Future,
    pin::pin,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll, Waker},
};
struct Fake {
    result: Result<Admission, LimiterUnavailable>,
    calls: AtomicUsize,
}
impl AttemptLimiter for Fake {
    async fn consume(&self, _attempt: &Attempt) -> Result<Admission, LimiterUnavailable> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.result
    }
}
fn ready<F: Future>(future: F) -> F::Output {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("in-memory fake unexpectedly suspended"),
    }
}
#[test]
fn unavailable_or_limited_enforcement_never_grants_or_retries() {
    let request = Attempt::new(vec![Budget {
        key: [1; 32],
        rule: BudgetRule::new(1, 1000).unwrap(),
    }])
    .unwrap();
    for (result, expected) in [
        (Ok(Admission::Allowed), Ok(())),
        (
            Ok(Admission::Limited {
                retry_after_ms: 100,
            }),
            Err(AdmissionFailure::Limited {
                retry_after_ms: 100,
            }),
        ),
        (Err(LimiterUnavailable), Err(AdmissionFailure::Unavailable)),
    ] {
        let limiter = Fake {
            result,
            calls: AtomicUsize::new(0),
        };
        assert_eq!(ready(require_admission(&limiter, &request)), expected);
        assert_eq!(limiter.calls.load(Ordering::Relaxed), 1);
    }
}
