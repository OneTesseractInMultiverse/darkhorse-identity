use super::*;
use std::{
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};
#[derive(Clone)]
struct Fake {
    calls: Arc<Mutex<Vec<&'static str>>>,
    fail: Option<&'static str>,
}
impl Fake {
    fn step(&self, step: &'static str) -> Result<(), Error> {
        self.calls.lock().unwrap().push(step);
        if self.fail == Some(step) {
            Err(Error::Unavailable)
        } else {
            Ok(())
        }
    }
}
impl Entropy for Fake {
    fn key(&self) -> Result<Prepared, Error> {
        self.step("entropy")?;
        Ok(Prepared {
            value: "single-reveal".into(),
            verifier: Verifier {
                id: CredentialId::from_u128(3).unwrap(),
                digest: [4; 32],
            },
        })
    }
}
impl Store for Fake {
    async fn options(&self, _: [u8; 32], _: Option<ResourceId>) -> Result<Options, Error> {
        unreachable!()
    }
    async fn list(&self, _: [u8; 32], _: Option<CredentialId>) -> Result<Page, Error> {
        unreachable!()
    }
    async fn revoke(&self, _: [u8; 32], _: CredentialId) -> Result<(), Error> {
        unreachable!()
    }
    async fn preflight(&self, actor: [u8; 32], _: &Request) -> Result<(), Error> {
        assert_eq!(actor, [1; 32]);
        self.step("preflight")
    }
    async fn issue(
        &self,
        actor: [u8; 32],
        request: &Request,
        verifier: Verifier,
    ) -> Result<Record, Error> {
        assert_eq!(actor, [1; 32]);
        assert_eq!(verifier.digest, [4; 32]);
        self.step("commit")?;
        Ok(Record {
            id: verifier.id,
            name: request.name().as_str().into(),
            application: request.application(),
            created_ms: 0,
            expires_ms: None,
            active: true,
            grants: vec![],
        })
    }
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("source-only fake"),
    }
}
#[test]
fn authorize_before_entropy_and_deliver_only_after_commit_without_retries() {
    let request = Request::new(
        "worker",
        ApplicationId::from_u128(1).unwrap(),
        0,
        darkhorse_domain::personal_keys::Expiration::Never,
        vec![darkhorse_domain::personal_keys::Selection {
            resource: ResourceId::from_u128(2).unwrap(),
            capabilities: darkhorse_domain::authorization::CapabilitySelection::All,
        }],
    )
    .unwrap();
    for (fail, expected) in [
        (Some("preflight"), vec!["preflight"]),
        (Some("entropy"), vec!["preflight", "entropy"]),
        (Some("commit"), vec!["preflight", "entropy", "commit"]),
        (None, vec!["preflight", "entropy", "commit"]),
    ] {
        let fake = Fake {
            calls: Arc::default(),
            fail,
        };
        let service = Service {
            store: fake.clone(),
            entropy: fake.clone(),
        };
        let result = ready(service.create([1; 32], &request));
        assert_eq!(*fake.calls.lock().unwrap(), expected);
        if fail.is_none() {
            assert_eq!(result.unwrap().secret, "single-reveal");
        } else {
            assert_eq!(result.err(), Some(Error::Unavailable));
        }
    }
}
