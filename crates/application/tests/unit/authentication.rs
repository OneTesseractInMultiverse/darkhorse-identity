use super::*;
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
    fn call(&self, name: &'static str) -> Result<(), AuthError> {
        self.calls.lock().unwrap().push(name);
        if self.fail == Some(name) {
            Err(AuthError::Unavailable)
        } else {
            Ok(())
        }
    }
}
fn candidate() -> Candidate {
    Candidate {
        principal: PrincipalId::from_u128(1).unwrap(),
        credential: CredentialId::from_u128(2).unwrap(),
        epoch: 3,
        active: true,
        verifier: "test-only".into(),
    }
}
impl LoginAdmission for Dependencies {
    async fn admit(&self, email: &str) -> Result<(), AuthError> {
        assert_eq!(email, "admin@example.com");
        self.call("admit")
    }
}
impl PasswordVerification for Dependencies {
    async fn verify(&self, password: &str, verifier: Option<&str>) -> Result<bool, AuthError> {
        assert_eq!(password, " test passphrase ");
        assert_eq!(verifier, self.exists.then_some("test-only"));
        self.call("verify")?;
        Ok(self.matches)
    }
}
impl SessionEntropy for Dependencies {
    fn generate(&self) -> Result<SessionSecret, AuthError> {
        self.call("entropy")?;
        Ok(SessionSecret {
            value: "fresh".into(),
            digest: [1; 32],
        })
    }
}
impl AuthenticationStore for Dependencies {
    async fn candidate(&self, _: &str) -> Result<Option<Candidate>, AuthError> {
        self.call("lookup")?;
        Ok(self.exists.then(|| Candidate {
            active: self.active,
            ..candidate()
        }))
    }
    async fn establish(
        &self,
        c: &Candidate,
        digest: [u8; 32],
        previous: Option<[u8; 32]>,
    ) -> Result<SessionView, AuthError> {
        assert_eq!(c.epoch, 3);
        assert_eq!(digest, [1; 32]);
        assert_eq!(previous, Some([2; 32]));
        self.call("establish")?;
        Ok(SessionView {
            principal: c.principal,
            name: "Ada".into(),
            locale: None,
        })
    }
    async fn session(&self, digest: [u8; 32]) -> Result<SessionView, AuthError> {
        assert_eq!(digest, [3; 32]);
        self.call("session")?;
        Ok(SessionView {
            principal: candidate().principal,
            name: "Ada".into(),
            locale: None,
        })
    }
    async fn logout(&self, digest: [u8; 32]) -> Result<(), AuthError> {
        assert_eq!(digest, [3; 32]);
        self.call("logout")
    }
}

#[test]
fn browser_facade_preserves_store_results_and_login_contract() {
    let service = Service {
        store: Dependencies::new(),
        admission: Dependencies::new(),
        passwords: Dependencies::new(),
        entropy: Dependencies::new(),
    };
    assert!(run(service.login("admin@example.com", " test passphrase ", Some([2; 32]))).is_ok());
    assert_eq!(run(service.session([3; 32])).unwrap().name, "Ada");
    assert_eq!(run(service.logout([3; 32])), Ok(()));
    assert_eq!(
        *service.store.calls.lock().unwrap(),
        ["lookup", "establish", "session", "logout"]
    );
}
fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("fake suspended"),
    }
}
fn sign_in(d: &Dependencies) -> Result<SignedIn, AuthError> {
    run(login(
        d,
        d,
        d,
        d,
        " Admin@Example.COM ",
        " test passphrase ",
        Some([2; 32]),
    ))
}
#[test]
fn success_and_every_dependency_failure_stop_in_order() {
    let order = ["admit", "lookup", "verify", "entropy", "establish"];
    let d = Dependencies::new();
    let result = sign_in(&d).unwrap();
    assert_eq!(result.secret.value, "fresh");
    assert_eq!(result.view.name, "Ada");
    assert_eq!(*d.calls.lock().unwrap(), order);
    for (index, step) in order.iter().enumerate() {
        let d = Dependencies {
            fail: Some(step),
            ..Dependencies::new()
        };
        assert!(matches!(sign_in(&d), Err(AuthError::Unavailable)));
        assert_eq!(*d.calls.lock().unwrap(), order[..=index]);
    }
}
#[test]
fn unknown_inactive_and_wrong_password_all_verify_and_never_issue() {
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
        assert!(matches!(sign_in(&d), Err(AuthError::Denied)));
        assert_eq!(*d.calls.lock().unwrap(), ["admit", "lookup", "verify"]);
    }
}
#[test]
fn invalid_input_has_no_effects() {
    for (email, password) in [("invalid", "test"), ("admin@example.com", "")] {
        let d = Dependencies::new();
        assert!(matches!(
            run(login(&d, &d, &d, &d, email, password, None)),
            Err(AuthError::Denied)
        ));
        assert!(d.calls.lock().unwrap().is_empty());
    }
}
