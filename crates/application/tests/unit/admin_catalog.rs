use super::*;
use crate::registration::NewSecret;
use darkhorse_domain::registration::Label;
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
    fn call(&self, name: &'static str) -> Result<(), Error> {
        self.calls.lock().unwrap().push(name);
        if self.fail == Some(name) {
            Err(Error::Unavailable)
        } else {
            Ok(())
        }
    }
}
fn target() -> Target {
    Target::Role(RoleId::from_u128(1).unwrap())
}
fn view() -> View {
    View {
        item: Item::Role(RoleSummary {
            id: RoleId::from_u128(1).unwrap(),
            name: "Reader".into(),
        }),
        applications: vec![],
        capabilities: vec![],
        policy_revision: 7,
    }
}
impl CatalogStore for Fake {
    async fn list(&self, actor: [u8; 32], _: List, q: Query) -> Result<Page, Error> {
        assert_eq!(actor, [1; 32]);
        assert_eq!(q.limit, 25);
        self.call("list")?;
        Ok(Page {
            items: vec![],
            next: None,
            policy_revision: 7,
        })
    }
    async fn view(&self, actor: [u8; 32], _: Target) -> Result<View, Error> {
        assert_eq!(actor, [1; 32]);
        self.call("view")?;
        Ok(view())
    }
    async fn preflight(&self, actor: [u8; 32], revision: u64, _: &Change) -> Result<(), Error> {
        assert_eq!(actor, [1; 32]);
        assert_eq!(revision, 7);
        self.call("preflight")
    }
    async fn execute(
        &self,
        actor: [u8; 32],
        revision: u64,
        change: Change,
        identifier: Option<NonZeroU128>,
    ) -> Result<Written, Error> {
        assert_eq!(actor, [1; 32]);
        assert_eq!(revision, 7);
        assert_eq!(identifier.is_some(), change.needs_identifier());
        self.call("execute")?;
        Ok(Written {
            target: target(),
            policy_revision: 8,
        })
    }
}
impl Entropy for Fake {
    fn identifier(&self) -> Result<NonZeroU128, Error> {
        self.call("identifier")?;
        Ok(NonZeroU128::new(1).unwrap())
    }
    fn secret(&self) -> Result<NewSecret, Error> {
        panic!("catalog commands must never generate credentials")
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
fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("source fakes are immediate"),
    }
}
#[test]
fn authority_precedes_entropy_and_no_failure_retries_or_continues() {
    for (fail, calls) in [
        (Some("preflight"), vec!["preflight"]),
        (Some("identifier"), vec!["preflight", "identifier"]),
        (Some("execute"), vec!["preflight", "identifier", "execute"]),
        (None, vec!["preflight", "identifier", "execute"]),
    ] {
        let service = service(fail);
        let result = run(service.write(
            [1; 32],
            7,
            Change::CreateRole {
                name: Label::new("Reader").unwrap(),
                application: None,
            },
        ));
        assert_eq!(result.is_ok(), fail.is_none());
        assert_eq!(*service.store.calls.lock().unwrap(), calls);
    }
    let service = service(None);
    assert_eq!(
        run(service.write(
            [1; 32],
            7,
            Change::RetireCapability(CapabilityId::from_u128(2).unwrap())
        ))
        .unwrap()
        .policy_revision,
        8
    );
    assert_eq!(
        *service.store.calls.lock().unwrap(),
        ["preflight", "execute"]
    );
}
#[test]
fn reads_forward_authority_and_storage_failures() {
    for fail in [None, Some("list"), Some("view")] {
        let service = service(fail);
        let result = run(service.list(
            [1; 32],
            List::Applications,
            Query {
                search: String::new(),
                active: None,
                after: None,
                limit: 25,
            },
        ));
        assert_eq!(result.is_ok(), fail != Some("list"));
        assert_eq!(
            run(service.view([1; 32], target())).is_ok(),
            fail != Some("view")
        );
        assert_eq!(*service.store.calls.lock().unwrap(), ["list", "view"]);
    }
}
#[test]
fn cursor_identity_preserves_each_catalog_reference_without_crossing_types() {
    let app = ApplicationId::from_u128(1).unwrap();
    let res = ResourceId::from_u128(3).unwrap();
    let items = [
        Item::Application(ApplicationRecord {
            id: app,
            name: "App".into(),
            owner: PrincipalId::from_u128(9).unwrap(),
            owner_email: "owner@example.com".into(),
            active: true,
            revision: 0,
        }),
        Item::Client(ClientSummary {
            id: ClientId::from_u128(2).unwrap(),
            application: app,
            name: "Web".into(),
            active: true,
            revision: 0,
        }),
        Item::Resource(ResourceRecord {
            id: res,
            application: app,
            name: "API".into(),
            audience: "urn:api".into(),
        }),
        Item::Scope(ScopeRecord {
            id: ScopeId::from_u128(4).unwrap(),
            application: app,
            resource: res,
            name: "read".into(),
        }),
        Item::Capability(CapabilitySummary {
            id: CapabilityId::from_u128(5).unwrap(),
            key: "read".into(),
            meaning: "Read records".into(),
            retired: false,
        }),
        Item::Role(RoleSummary {
            id: RoleId::from_u128(6).unwrap(),
            name: "Reader".into(),
        }),
    ];
    for (index, item) in items.into_iter().enumerate() {
        assert_eq!(item.id().get(), (index + 1) as u128);
    }
}
