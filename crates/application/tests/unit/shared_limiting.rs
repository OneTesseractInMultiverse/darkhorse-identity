use super::*;
use darkhorse_domain::limiting::{Budget, BudgetRule};
use std::{
    collections::VecDeque,
    future::Future,
    pin::pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};
struct Fake {
    states: Mutex<VecDeque<Result<Enforcement, LimiterUnavailable>>>,
    writes: Mutex<VecDeque<Result<Commit, LimiterUnavailable>>>,
    events: Mutex<Vec<&'static str>>,
    used: u32,
    snapshot_failure: bool,
    prune_failure: bool,
}
fn state() -> Enforcement {
    Enforcement {
        generation: Generation::new(1, [1; 16]).unwrap(),
        active: true,
        not_before_ms: 0,
        now_ms: 2000,
        identity: Some(ServerIdentity {
            run: [1; 20],
            replication: [2; 20],
        }),
    }
}
fn fake(writes: Vec<Result<Commit, LimiterUnavailable>>) -> Fake {
    Fake {
        states: Mutex::new(VecDeque::from([Ok(state()), Ok(state())])),
        writes: Mutex::new(writes.into()),
        events: Mutex::new(vec![]),
        used: 0,
        snapshot_failure: false,
        prune_failure: false,
    }
}
fn request() -> Attempt {
    Attempt::new(vec![Budget {
        key: [1; 32],
        rule: BudgetRule::new(1, 1000).unwrap(),
    }])
    .unwrap()
}
fn ready<F: Future>(future: F) -> F::Output {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fake unexpectedly suspended"),
    }
}
impl EnforcementAuthority for Fake {
    async fn read(&self) -> Result<Enforcement, LimiterUnavailable> {
        self.events.lock().unwrap().push("authority");
        self.states.lock().unwrap().pop_front().unwrap()
    }
}
impl CounterStore for Fake {
    async fn snapshot(&self, _: Enforcement, a: &Attempt) -> Result<Snapshot, LimiterUnavailable> {
        self.events.lock().unwrap().push("snapshot");
        if self.snapshot_failure {
            return Err(LimiterUnavailable);
        }
        Ok(Snapshot {
            counters: vec![if self.used == 0 {
                None
            } else {
                Some(Counter {
                    rule: a.budgets()[0].rule,
                    used: self.used,
                    started_ms: 2000,
                    last_ms: 2000,
                })
            }],
            redis_ms: 2000,
            previous_ms: 2000,
        })
    }
    async fn apply(
        &self,
        _: Enforcement,
        _: &Attempt,
        _: &Snapshot,
        c: &[Counter],
        _: u64,
    ) -> Result<Commit, LimiterUnavailable> {
        assert_eq!(c[0].used, 1);
        self.events.lock().unwrap().push("apply");
        self.writes.lock().unwrap().pop_front().unwrap()
    }
    async fn prune(&self, _: Enforcement) -> Result<(), LimiterUnavailable> {
        self.events.lock().unwrap().push("prune");
        if self.prune_failure {
            return Err(LimiterUnavailable);
        }
        Ok(())
    }
}
#[test]
fn allowed_requires_atomic_charge_and_a_second_authority_check() {
    let f = fake(vec![Ok(Commit::Applied)]);
    assert_eq!(ready(consume(&f, &f, &request())), Ok(Admission::Allowed));
    assert_eq!(
        *f.events.lock().unwrap(),
        ["authority", "snapshot", "apply", "authority"]
    );
    for result in [
        Err(LimiterUnavailable),
        Ok(Enforcement {
            active: false,
            ..state()
        }),
        Ok(Enforcement {
            generation: Generation::new(2, [2; 16]).unwrap(),
            ..state()
        }),
    ] {
        let f = fake(vec![Ok(Commit::Applied)]);
        f.states.lock().unwrap()[1] = result;
        assert_eq!(ready(consume(&f, &f, &request())), Err(LimiterUnavailable));
    }
}
#[test]
fn uncertain_write_is_never_retried_and_limited_state_never_writes() {
    let f = fake(vec![Err(LimiterUnavailable)]);
    assert_eq!(ready(consume(&f, &f, &request())), Err(LimiterUnavailable));
    assert_eq!(
        *f.events.lock().unwrap(),
        ["authority", "snapshot", "apply"]
    );
    let mut f = fake(vec![]);
    f.used = 1;
    assert_eq!(
        ready(consume(&f, &f, &request())),
        Ok(Admission::Limited {
            retry_after_ms: 1000
        })
    );
    assert_eq!(*f.events.lock().unwrap(), ["authority", "snapshot"]);
}

#[test]
fn invalid_authority_counters_and_clocks_stop_before_any_charge() {
    for state in [
        Err(LimiterUnavailable),
        Ok(Enforcement {
            active: false,
            ..state()
        }),
        Ok(Enforcement {
            now_ms: 4000,
            ..state()
        }),
    ] {
        let f = fake(vec![]);
        f.states.lock().unwrap()[0] = state;
        assert_eq!(ready(consume(&f, &f, &request())), Err(LimiterUnavailable));
        assert!(!f.events.lock().unwrap().contains(&"apply"));
    }
    let mut f = fake(vec![]);
    f.used = 2;
    assert_eq!(ready(consume(&f, &f, &request())), Err(LimiterUnavailable));
}

#[test]
fn snapshot_or_cleanup_outages_do_not_trigger_another_write() {
    let mut f = fake(vec![]);
    f.snapshot_failure = true;
    assert_eq!(ready(consume(&f, &f, &request())), Err(LimiterUnavailable));
    assert_eq!(*f.events.lock().unwrap(), ["authority", "snapshot"]);
    let mut f = fake(vec![Ok(Commit::Full)]);
    f.prune_failure = true;
    assert_eq!(ready(consume(&f, &f, &request())), Err(LimiterUnavailable));
    assert_eq!(
        *f.events.lock().unwrap(),
        ["authority", "snapshot", "apply", "prune"]
    );
}
#[test]
fn only_known_nonmutating_conflicts_retry_and_retries_are_bounded() {
    for first in [Commit::Conflict, Commit::Full] {
        let f = fake(vec![Ok(first), Ok(Commit::Applied)]);
        assert_eq!(ready(consume(&f, &f, &request())), Ok(Admission::Allowed));
        assert_eq!(
            f.events
                .lock()
                .unwrap()
                .iter()
                .filter(|&&e| e == "apply")
                .count(),
            2
        );
        assert_eq!(
            f.events.lock().unwrap().contains(&"prune"),
            first == Commit::Full
        );
    }
    let f = fake(vec![Ok(Commit::Conflict); 3]);
    assert_eq!(ready(consume(&f, &f, &request())), Err(LimiterUnavailable));
    assert_eq!(
        f.events
            .lock()
            .unwrap()
            .iter()
            .filter(|&&e| e == "apply")
            .count(),
        3
    );
}
