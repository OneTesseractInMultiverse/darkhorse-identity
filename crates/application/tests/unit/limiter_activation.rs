use super::*;
use std::sync::Mutex;
struct Fixture {
    events: Mutex<Vec<&'static str>>,
    failure: Option<&'static str>,
    active: bool,
}
fn id() -> OperationId {
    OperationId::from_u128(1).unwrap()
}
fn state() -> Enforcement {
    Enforcement {
        generation: Generation::new(1, [1; 16]).unwrap(),
        active: false,
        not_before_ms: 1,
        now_ms: 2,
        identity: None,
    }
}
fn identity() -> ServerIdentity {
    ServerIdentity {
        run: [1; 20],
        replication: [2; 20],
    }
}
impl Journal for Fixture {
    async fn prepare(&self, _: OperationId) -> Result<Enforcement, Error> {
        self.events.lock().unwrap().push("intent");
        if self.failure == Some("intent") {
            return Err(Error::Unavailable);
        }
        Ok(Enforcement {
            active: self.active,
            ..state()
        })
    }
    async fn complete(
        &self,
        operation: OperationId,
        generation: Generation,
        server: ServerIdentity,
    ) -> Result<(), Error> {
        assert_eq!(operation, id());
        assert_eq!(generation, state().generation);
        assert_eq!(server, identity());
        self.events.lock().unwrap().push("completion");
        if self.failure == Some("completion") {
            Err(Error::Unavailable)
        } else {
            Ok(())
        }
    }
    async fn inspect(&self, _: OperationId) -> Result<Attempt, Error> {
        unreachable!("activation never retries or reconciles implicitly")
    }
}
impl Initializer for Fixture {
    async fn initialize(&self, value: Enforcement) -> Result<ServerIdentity, Error> {
        assert_eq!(value, state());
        self.events.lock().unwrap().push("redis");
        if self.failure == Some("redis") {
            Err(Error::Unavailable)
        } else {
            Ok(identity())
        }
    }
}
#[test]
fn records_intent_before_external_effect_and_requires_committed_completion() {
    let fixture = Fixture {
        events: Mutex::new(vec![]),
        failure: None,
        active: false,
    };
    assert_eq!(run(activate(&fixture, &fixture, id())), Ok(()));
    assert_eq!(
        *fixture.events.lock().unwrap(),
        vec!["intent", "redis", "completion"]
    );
}
#[test]
fn intent_failure_or_unsafe_snapshot_never_touches_redis() {
    for (failure, active, error) in [
        (Some("intent"), false, Error::Unavailable),
        (None, true, Error::NotReady),
    ] {
        let fixture = Fixture {
            events: Mutex::new(vec![]),
            failure,
            active,
        };
        assert_eq!(run(activate(&fixture, &fixture, id())), Err(error));
        assert_eq!(*fixture.events.lock().unwrap(), vec!["intent"]);
    }
}
#[test]
fn lost_external_or_completion_result_is_uncertain_and_never_retried() {
    for (failure, events) in [
        ("redis", vec!["intent", "redis"]),
        ("completion", vec!["intent", "redis", "completion"]),
    ] {
        let fixture = Fixture {
            events: Mutex::new(vec![]),
            failure: Some(failure),
            active: false,
        };
        assert_eq!(
            run(activate(&fixture, &fixture, id())),
            Err(Error::Uncertain)
        );
        assert_eq!(*fixture.events.lock().unwrap(), events);
    }
}

fn run<T>(future: impl std::future::Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
    {
        std::task::Poll::Ready(value) => value,
        std::task::Poll::Pending => panic!("source-only fakes must finish immediately"),
    }
}
