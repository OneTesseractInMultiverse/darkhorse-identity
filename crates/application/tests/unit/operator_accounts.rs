use super::*;
use darkhorse_domain::{
    AccountStatus,
    directory::Profile,
    identity::{CredentialId, PrincipalId},
    operator_accounts::Operation,
};
use std::{
    sync::Mutex,
    task::{Context, Poll, Waker},
};
struct Dependencies {
    calls: Mutex<Vec<&'static str>>,
    fail: Option<&'static str>,
    exists: bool,
    active: bool,
    matches: bool,
}
impl Dependencies {
    fn new() -> Self {
        Self {
            calls: Mutex::new(vec![]),
            fail: None,
            exists: true,
            active: true,
            matches: true,
        }
    }
    fn call(&self, name: &'static str) -> Result<(), Error> {
        self.calls.lock().unwrap().push(name);
        if self.fail == Some(name) {
            Err(Error::Unavailable)
        } else {
            Ok(())
        }
    }
}
fn id() -> OperationId {
    OperationId::from_u128(9).unwrap()
}
fn target() -> PrincipalId {
    PrincipalId::from_u128(1).unwrap()
}
fn request() -> Request {
    Request::new(Operation::Show(target()), None).unwrap()
}
impl LoginAdmission for Dependencies {
    async fn admit(&self, email: &str) -> Result<(), AuthError> {
        assert_eq!(email, "admin@example.com");
        self.call("admit")
            .map_err(|_| AuthError::Limited { retry_after_ms: 17 })
    }
}
impl PasswordVerification for Dependencies {
    async fn verify(&self, password: &str, verifier: Option<&str>) -> Result<bool, AuthError> {
        assert_eq!(password, " test passphrase ");
        assert_eq!(verifier, self.exists.then_some("source-only"));
        self.call("verify").map_err(|_| AuthError::Unavailable)?;
        Ok(self.matches)
    }
}
impl Store for Dependencies {
    type Request = Request;
    type Outcome = Outcome;
    async fn candidate(&self, _: &str) -> Result<Option<CandidateAt>, Error> {
        self.call("candidate")?;
        Ok(self.exists.then(|| CandidateAt {
            credential: Candidate {
                principal: target(),
                credential: CredentialId::from_u128(2).unwrap(),
                epoch: 3,
                active: self.active,
                verifier: "source-only".into(),
            },
            observed_ms: 4,
        }))
    }
    async fn denied(&self, correlation: OperationId, request: &Request) -> Result<(), Error> {
        assert_eq!(correlation, id());
        assert_eq!(request.operation(), Operation::Show(target()));
        self.call("denied")
    }
    async fn execute(&self, proof: Verified) -> Result<Outcome, Error> {
        self.call("execute")?;
        assert_eq!(proof.id(), id());
        assert_eq!(proof.request().operation(), Operation::Show(target()));
        assert_eq!(proof.candidate().credential.epoch, 3);
        assert_eq!(proof.candidate().observed_ms, 4);
        Ok(Outcome {
            changed: false,
            account: AccountRecord {
                id: target(),
                profile: Profile::new("a@example.com", "First", "Last").unwrap(),
                status: AccountStatus::Active,
                credential_epoch: 0,
                revision: 0,
                administrator: true,
                eligible_administrator: true,
            },
        })
    }
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fake suspended"),
    }
}
fn invoke(d: &Dependencies) -> Result<Outcome, Error> {
    ready(run(
        d,
        d,
        d,
        id(),
        request(),
        " Admin@Example.COM ",
        " test passphrase ",
    ))
}
#[test]
fn admission_precedes_password_work_and_current_store_decision_is_required() {
    let order = ["admit", "candidate", "verify", "execute"];
    let d = Dependencies::new();
    assert!(!invoke(&d).unwrap().changed);
    assert_eq!(*d.calls.lock().unwrap(), order);
    for (index, step) in order.iter().enumerate() {
        let d = Dependencies {
            fail: Some(step),
            ..Dependencies::new()
        };
        assert!(invoke(&d).is_err());
        assert_eq!(*d.calls.lock().unwrap(), order[..=index]);
    }
}
#[test]
fn absent_inactive_and_wrong_credentials_take_verification_path_and_record_anonymous_denial() {
    for d in [
        Dependencies {
            exists: false,
            ..Dependencies::new()
        },
        Dependencies {
            active: false,
            ..Dependencies::new()
        },
        Dependencies {
            matches: false,
            ..Dependencies::new()
        },
    ] {
        assert!(matches!(invoke(&d), Err(Error::Denied)));
        assert_eq!(
            *d.calls.lock().unwrap(),
            ["admit", "candidate", "verify", "denied"]
        );
    }
    let d = Dependencies {
        matches: false,
        fail: Some("denied"),
        ..Dependencies::new()
    };
    assert!(matches!(invoke(&d), Err(Error::Unavailable)));
}
#[test]
fn invalid_input_has_no_effect_and_admission_failures_preserve_retry_delay() {
    let d = Dependencies::new();
    for (email, password) in [("invalid", "test"), ("admin@example.com", "")] {
        assert!(matches!(
            ready(run(&d, &d, &d, id(), request(), email, password)),
            Err(Error::Denied)
        ));
    }
    assert!(d.calls.lock().unwrap().is_empty());
    let d = Dependencies {
        fail: Some("admit"),
        ..Dependencies::new()
    };
    assert!(matches!(
        invoke(&d),
        Err(Error::Limited { retry_after_ms: 17 })
    ));
    assert_eq!(auth_error(AuthError::Denied), Error::Denied);
}
