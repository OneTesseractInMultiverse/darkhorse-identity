use super::*;
use std::{
    collections::VecDeque,
    future::Future,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

struct Fake(Mutex<VecDeque<Result<u64, Error>>>);
impl AuthorizationMaintenance for Fake {
    async fn prune_expired_authorization_requests(&self) -> Result<u64, Error> {
        self.0
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected extra cleanup batch")
    }
}
fn run(future: impl Future<Output = Result<(), Error>>) -> Result<(), Error> {
    match std::pin::pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("in-memory cleanup fake must be immediately ready"),
    }
}

#[test]
fn request_cleanup_stops_after_a_partial_batch_and_bounds_full_sweeps() {
    for count in [0, 1, u64::from(AUTHORIZATION_REQUEST_CLEANUP_BATCH) - 1] {
        let fake = Fake(Mutex::new(VecDeque::from([
            Ok(u64::from(AUTHORIZATION_REQUEST_CLEANUP_BATCH)),
            Ok(count),
        ])));
        assert_eq!(run(sweep_expired_authorization_requests(&fake)), Ok(()));
        assert!(fake.0.lock().unwrap().is_empty());
    }

    let fake = Fake(Mutex::new(VecDeque::from([
        Ok(u64::from(AUTHORIZATION_REQUEST_CLEANUP_BATCH)),
        Err(Error::Unavailable),
        Ok(0),
    ])));
    assert_eq!(
        run(sweep_expired_authorization_requests(&fake)),
        Err(Error::Unavailable)
    );
    assert_eq!(fake.0.lock().unwrap().len(), 1);

    let fake = Fake(Mutex::new(VecDeque::from(
        [Ok(u64::from(AUTHORIZATION_REQUEST_CLEANUP_BATCH)); 11],
    )));
    assert_eq!(run(sweep_expired_authorization_requests(&fake)), Ok(()));
    assert_eq!(fake.0.lock().unwrap().len(), 1);
}
