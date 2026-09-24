use super::*;
use std::sync::Mutex;
struct Fixture {
    events: Mutex<Vec<&'static str>>,
    prepare: Result<(), Error>,
    complete: Result<u64, Error>,
}
fn intent() -> Intent {
    Intent {
        id: OperationId::from_u128(1).unwrap(),
        issuer: "https://issuer.example".into(),
        kid: "public".into(),
        kind: Kind::Activate,
        expected_revision: 1,
    }
}
impl Journal for Fixture {
    async fn prepare_signing(&self, value: &Intent) -> Result<(), Error> {
        assert_eq!(value, &intent());
        self.events.lock().unwrap().push("intent");
        self.prepare
    }
    async fn complete_signing(
        &self,
        value: &Intent,
        digest: [u8; 32],
        key: Option<WrappedKey>,
    ) -> Result<u64, Error> {
        assert_eq!(value, &intent());
        assert_eq!(digest, [1; 32]);
        assert!(key.is_none());
        self.events.lock().unwrap().push("complete");
        self.complete
    }
    async fn inspect_signing(&self, _: OperationId) -> Result<Attempt, Error> {
        panic!("no automatic inspection or retry")
    }
}
#[test]
fn durable_intent_precedes_mutation_and_errors_never_retry() {
    for (prepare, complete, events, result) in [
        (Ok(()), Ok(2), vec!["intent", "complete"], Ok(2)),
        (
            Err(Error::Rejected(KeyError::Unavailable)),
            Ok(2),
            vec!["intent"],
            Err(Error::Rejected(KeyError::Unavailable)),
        ),
        (
            Err(Error::Uncertain),
            Ok(2),
            vec!["intent"],
            Err(Error::Uncertain),
        ),
        (
            Ok(()),
            Err(Error::Uncertain),
            vec!["intent", "complete"],
            Err(Error::Uncertain),
        ),
        (
            Ok(()),
            Err(Error::Rejected(KeyError::Conflict)),
            vec!["intent", "complete"],
            Err(Error::Rejected(KeyError::Conflict)),
        ),
    ] {
        let fixture = Fixture {
            events: Mutex::new(vec![]),
            prepare,
            complete,
        };
        let value = intent();
        let future = execute(&fixture, &value, [1; 32], None);
        let mut future = std::pin::pin!(future);
        assert_eq!(
            future
                .as_mut()
                .poll(&mut std::task::Context::from_waker(std::task::Waker::noop())),
            std::task::Poll::Ready(result)
        );
        assert_eq!(*fixture.events.lock().unwrap(), events);
    }
    assert_eq!(
        Error::from(KeyError::Invalid),
        Error::Rejected(KeyError::Invalid)
    );
}
