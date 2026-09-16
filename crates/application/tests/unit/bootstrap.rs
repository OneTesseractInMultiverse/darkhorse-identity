use super::*;
use std::{
    sync::Mutex,
    task::{Context, Poll, Waker},
};

struct Preparation {
    fail: bool,
    calls: Mutex<usize>,
}
impl CredentialPreparation for Preparation {
    async fn prepare(&self, password: &str) -> Result<PreparedCredential, BootstrapError> {
        *self.calls.lock().unwrap() += 1;
        assert_eq!(password, " a long passphrase ");
        if self.fail {
            return Err(BootstrapError::SecretPreparation);
        }
        Ok(PreparedCredential {
            principal_id: PrincipalId::from_u128(1).unwrap(),
            credential_id: CredentialId::from_u128(2).unwrap(),
            verifier: "test verifier".to_owned(),
        })
    }
}
struct Store {
    error: Option<BootstrapError>,
    records: Mutex<Vec<NewAdministrator>>,
}
impl BootstrapStore for Store {
    async fn bootstrap(
        &self,
        administrator: NewAdministrator,
    ) -> Result<PrincipalId, BootstrapError> {
        let id = administrator.credential.principal_id;
        self.records.lock().unwrap().push(administrator);
        self.error.map_or(Ok(id), Err)
    }
}
fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fake unexpectedly suspended"),
    }
}
fn request() -> BootstrapRequest<'static> {
    BootstrapRequest {
        email: "Admin@Example.com",
        first_name: "Ada",
        last_name: "Lovelace",
        password: " a long passphrase ",
    }
}
fn dependencies(error: Option<BootstrapError>, fail: bool) -> (Store, Preparation) {
    (
        Store {
            error,
            records: Mutex::new(vec![]),
        },
        Preparation {
            fail,
            calls: Mutex::new(0),
        },
    )
}

#[test]
fn validates_before_effects_and_never_persists_on_preparation_failure() {
    for request in [
        BootstrapRequest {
            email: "invalid",
            ..request()
        },
        BootstrapRequest {
            password: "short",
            ..request()
        },
    ] {
        let (store, prep) = dependencies(None, false);
        assert!(matches!(
            run(bootstrap(&store, &prep, request)),
            Err(BootstrapError::Invalid(_))
        ));
        assert_eq!(*prep.calls.lock().unwrap(), 0);
        assert!(store.records.lock().unwrap().is_empty());
    }
    let (store, prep) = dependencies(None, true);
    assert_eq!(
        run(bootstrap(&store, &prep, request())),
        Err(BootstrapError::SecretPreparation)
    );
    assert!(store.records.lock().unwrap().is_empty());
}

#[test]
fn persists_exactly_one_prepared_administrator_and_propagates_store_outcomes() {
    for error in [
        None,
        Some(BootstrapError::AlreadyInitialized),
        Some(BootstrapError::Storage),
    ] {
        let (store, prep) = dependencies(error, false);
        assert_eq!(
            run(bootstrap(&store, &prep, request())),
            error.map_or(Ok(PrincipalId::from_u128(1).unwrap()), Err)
        );
        assert_eq!(*prep.calls.lock().unwrap(), 1);
        let records = store.records.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].profile.email_key(), "admin@example.com");
        assert_eq!(records[0].credential.verifier, "test verifier");
    }
}
