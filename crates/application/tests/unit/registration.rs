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
    fn call(&self, step: &'static str) -> Result<(), RegistrationError> {
        self.calls.lock().unwrap().push(step);
        if self.fail == Some(step) {
            Err(RegistrationError::Unavailable)
        } else {
            Ok(())
        }
    }
}
fn record() -> Record {
    Record::Application(ApplicationRecord {
        id: ApplicationId::from_u128(1).unwrap(),
        name: "Portal".into(),
        owner: PrincipalId::from_u128(2).unwrap(),
        owner_email: "owner@example.com".into(),
        active: true,
        revision: 0,
    })
}
impl RegistrationStore for Fake {
    async fn preflight(&self, actor: [u8; 32], _: &Command) -> Result<(), RegistrationError> {
        assert_eq!(actor, [1; 32]);
        self.call("preflight")
    }
    async fn execute(
        &self,
        _: [u8; 32],
        command: &Command,
        prepared: Prepared,
    ) -> Result<Record, RegistrationError> {
        assert_eq!(prepared.identifier.is_some(), command.needs_identifier());
        assert_eq!(prepared.secret.is_some(), command.needs_secret());
        if let Some(secret) = prepared.secret {
            assert_eq!(secret.digest, [3; 32]);
        }
        self.call("commit")?;
        Ok(record())
    }
    async fn read(&self, _: [u8; 32], _: ReadTarget) -> Result<Record, RegistrationError> {
        self.call("read")?;
        Ok(record())
    }
}
impl Entropy for Fake {
    fn identifier(&self) -> Result<NonZeroU128, RegistrationError> {
        self.call("identifier")?;
        Ok(NonZeroU128::new(1).unwrap())
    }
    fn secret(&self) -> Result<NewSecret, RegistrationError> {
        self.call("secret")?;
        Ok(NewSecret {
            value: "reveal-once".into(),
            verifier: SecretVerifier {
                id: ClientSecretId::from_u128(3).unwrap(),
                digest: [3; 32],
            },
        })
    }
}
fn service(fail: Option<&'static str>) -> Service<Fake, Fake> {
    let fake = Fake {
        calls: Arc::default(),
        fail,
    };
    Service {
        store: fake.clone(),
        entropy: fake,
    }
}
fn commands() -> Vec<Command> {
    let application = ApplicationId::from_u128(1).unwrap();
    let client = ClientId::from_u128(2).unwrap();
    let app = ApplicationSpec {
        name: Label::new("Portal").unwrap(),
        owner: PrincipalId::from_u128(3).unwrap(),
        active: true,
    };
    let spec = ClientSpec::new(
        Label::new("Web").unwrap(),
        true,
        Redirects::from_validated_urls(vec!["https://app.example/callback".into()]).unwrap(),
        vec![],
        vec![],
        "client_secret_basic",
    )
    .unwrap();
    vec![
        Command::CreateApplication(app.clone()),
        Command::UpdateApplication {
            application,
            revision: 0,
            spec: app,
        },
        Command::CreateResource {
            application,
            name: Label::new("API").unwrap(),
        },
        Command::CreateScope {
            application,
            resource: ResourceId::from_u128(4).unwrap(),
            name: ScopeName::new("read").unwrap(),
        },
        Command::CreateClient {
            application,
            spec: spec.clone(),
        },
        Command::UpdateClient {
            application,
            client,
            revision: 0,
            spec,
        },
        Command::RotateSecret {
            application,
            client,
            revision: 0,
            overlap_seconds: 0,
        },
        Command::RetireSecret {
            application,
            client,
            secret: ClientSecretId::from_u128(5).unwrap(),
            revision: 0,
        },
    ]
}
fn run<T>(future: impl Future<Output = T>) -> T {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fake suspended"),
    }
}
#[test]
fn commands_generate_only_required_material_and_disclose_after_commit() {
    for command in commands() {
        let service = service(None);
        let mut expected = vec!["preflight"];
        if command.needs_identifier() {
            expected.push("identifier");
        }
        if command.needs_secret() {
            expected.push("secret");
        }
        expected.push("commit");
        let secret = command.needs_secret().then_some("reveal-once");
        assert_eq!(
            run(service.write([1; 32], command))
                .unwrap()
                .secret
                .as_deref(),
            secret
        );
        assert_eq!(*service.store.calls.lock().unwrap(), expected);
    }
}
#[test]
fn every_failure_stops_without_result_or_retry() {
    let order = ["preflight", "identifier", "secret", "commit"];
    for (index, step) in order.iter().enumerate() {
        let service = service(Some(step));
        assert!(matches!(
            run(service.write([1; 32], commands().remove(4))),
            Err(RegistrationError::Unavailable)
        ));
        assert_eq!(*service.store.calls.lock().unwrap(), order[..=index]);
    }
    for fail in [None, Some("read")] {
        let service = service(fail);
        assert_eq!(
            run(service.read(
                [1; 32],
                ReadTarget::Application(ApplicationId::from_u128(1).unwrap())
            ))
            .is_ok(),
            fail.is_none()
        );
        assert_eq!(*service.store.calls.lock().unwrap(), ["read"]);
    }
}
