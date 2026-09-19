use super::*;
use std::{
    collections::VecDeque,
    sync::Mutex,
    task::{Context, Poll, Waker},
};
struct Fake(Mutex<VecDeque<Result<u64, Error>>>);
impl RefreshMaintenance for Fake {
    async fn prune_refresh(&self) -> Result<u64, Error> {
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
fn refresh_maintenance_stops_on_partial_batch_or_failure_and_bounds_full_sweeps() {
    for count in [0, 1, 9] {
        let fake = Fake(Mutex::new(VecDeque::from([Ok(10), Ok(count)])));
        assert_eq!(run(sweep(&fake)), Ok(()));
        assert!(fake.0.lock().unwrap().is_empty());
    }
    let fake = Fake(Mutex::new(VecDeque::from([
        Ok(10),
        Err(Error::Unavailable),
        Ok(10),
    ])));
    assert_eq!(run(sweep(&fake)), Err(Error::Unavailable));
    assert_eq!(fake.0.lock().unwrap().len(), 1);
    let fake = Fake(Mutex::new(VecDeque::from([Ok(10); 11])));
    assert_eq!(run(sweep(&fake)), Ok(()));
    assert_eq!(fake.0.lock().unwrap().len(), 1);
}
