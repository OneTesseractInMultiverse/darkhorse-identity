use super::*;
use std::{
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};
#[derive(Clone)]
struct Fake {
    calls: Arc<Mutex<Vec<&'static str>>>,
    auth: Result<(), Error>,
    quota: Result<(), Error>,
}
impl Authenticate for Fake {
    async fn authenticate_introspection(&self, _: Credentials) -> Result<(), Error> {
        self.calls.lock().unwrap().push("authenticate");
        self.auth
    }
}
impl Budgets for Fake {
    async fn global(&self) -> Result<(), Error> {
        self.calls.lock().unwrap().push("global");
        self.quota
    }
    async fn caller(&self, _: Caller) -> Result<(), Error> {
        self.calls.lock().unwrap().push("caller");
        self.quota
    }
}
fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("source-defined fake suspended"),
    }
}
#[test]
fn authentication_precedes_caller_charging_and_failures_stop_work() {
    for auth in [Ok(()), Err(Error::InvalidClient), Err(Error::Unavailable)] {
        for quota in [
            Ok(()),
            Err(Error::Unavailable),
            Err(Error::Limited {
                retry_after_ms: 900,
            }),
        ] {
            let f = Fake {
                calls: Default::default(),
                auth,
                quota,
            };
            let s = Service {
                store: f.clone(),
                budgets: f.clone(),
            };
            assert_eq!(run(s.before()), quota);
            assert_eq!(*f.calls.lock().unwrap(), ["global"]);
            f.calls.lock().unwrap().clear();
            let credentials = Credentials {
                caller: Caller::Client(ClientId::from_u128(1).unwrap()),
                secret: [7; 32],
            };
            assert_eq!(run(s.authenticated(credentials)), auth.and(quota));
            let expected = if auth.is_ok() {
                vec!["authenticate", "caller"]
            } else {
                vec!["authenticate"]
            };
            assert_eq!(*f.calls.lock().unwrap(), expected);
        }
    }
}
