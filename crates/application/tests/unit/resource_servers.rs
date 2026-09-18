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
    fn step(&self, name: &'static str) -> Result<(), RegistrationError> {
        self.calls.lock().unwrap().push(name);
        if self.fail == Some(name) {
            Err(RegistrationError::Unavailable)
        } else {
            Ok(())
        }
    }
}
fn target() -> Target {
    Target {
        application: ApplicationId::from_u128(1).unwrap(),
        resource: ResourceId::from_u128(2).unwrap(),
    }
}
fn record() -> Record {
    Record {
        target: target(),
        revision: 0,
        active: true,
        secrets: vec![],
    }
}
impl Store for Fake {
    async fn preflight(&self, actor: [u8; 32], _: Command) -> Result<(), RegistrationError> {
        assert_eq!(actor, [1; 32]);
        self.step("preflight")
    }
    async fn execute(
        &self,
        _: [u8; 32],
        command: Command,
        secret: Option<Verifier>,
    ) -> Result<Record, RegistrationError> {
        assert_eq!(command.change.needs_secret(), secret.is_some());
        if let Some(secret) = secret {
            assert_eq!(secret.digest, [3; 32]);
        }
        self.step("commit")?;
        Ok(record())
    }
    async fn read(&self, _: [u8; 32], _: Target) -> Result<Record, RegistrationError> {
        self.step("read")?;
        Ok(record())
    }
}
impl Entropy for Fake {
    fn secret(&self) -> Result<NewSecret, RegistrationError> {
        self.step("secret")?;
        Ok(NewSecret {
            value: "once".into(),
            verifier: Verifier {
                id: CredentialId::from_u128(3).unwrap(),
                digest: [3; 32],
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
        Poll::Pending => panic!("in-memory future must be ready"),
    }
}
#[test]
fn authorization_precedes_entropy_and_only_committed_secrets_are_revealed() {
    for change in [
        Change::Register,
        Change::Rotate {
            revision: 0,
            overlap_seconds: 1,
        },
        Change::SetActive {
            revision: 0,
            active: false,
        },
    ] {
        for fail in [None, Some("preflight"), Some("secret"), Some("commit")] {
            let fake = Fake {
                calls: Arc::default(),
                fail,
            };
            let service = Service {
                store: fake.clone(),
                entropy: fake.clone(),
            };
            let result = ready(service.write(
                [1; 32],
                Command {
                    target: target(),
                    change,
                },
            ));
            let mut expected = vec!["preflight"];
            if fail != Some("preflight") {
                if change.needs_secret() {
                    expected.push("secret");
                }
                if fail != Some("secret") || !change.needs_secret() {
                    expected.push("commit");
                }
            }
            assert_eq!(*fake.calls.lock().unwrap(), expected);
            if fail.is_none() || (fail == Some("secret") && !change.needs_secret()) {
                assert_eq!(
                    result.unwrap().secret.as_deref(),
                    change.needs_secret().then_some("once")
                );
            } else {
                assert!(result.is_err());
            }
        }
    }
}
#[test]
fn reads_return_metadata_through_the_store_without_generating_a_secret() {
    for fail in [None, Some("read")] {
        let fake = Fake {
            calls: Arc::default(),
            fail,
        };
        let service = Service {
            store: fake.clone(),
            entropy: fake.clone(),
        };
        assert_eq!(
            ready(service.read([1; 32], target())).is_err(),
            fail.is_some()
        );
        assert_eq!(*fake.calls.lock().unwrap(), vec!["read"]);
    }
}
