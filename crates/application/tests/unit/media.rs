use super::*;
use std::{
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};
#[derive(Clone)]
struct Fake {
    calls: Arc<Mutex<Vec<&'static str>>>,
    fail: Option<&'static str>,
    empty: bool,
}
impl Fake {
    fn step(&self, event: &'static str) -> Result<(), Error> {
        self.calls.lock().unwrap().push(event);
        if self.fail == Some(event) {
            Err(Error::Unavailable)
        } else {
            Ok(())
        }
    }
}
fn id() -> AssetId {
    AssetId::from_u128(1).unwrap()
}
fn target() -> Target {
    Target {
        kind: Kind::Portrait,
        principal: None,
    }
}
impl Store for Fake {
    async fn reserve(&self, _: [u8; 32], target: Target, _: u64) -> Result<Ticket, Error> {
        self.step("reserve")?;
        Ok(Ticket { id: id(), target })
    }
    async fn attach(&self, _: [u8; 32], _: Ticket, _: u64, _: &Prepared) -> Result<u64, Error> {
        self.step("attach")?;
        Ok(1)
    }
    async fn remove(&self, _: [u8; 32], _: Target, _: u64) -> Result<u64, Error> {
        unreachable!()
    }
    async fn asset(&self, _: Option<[u8; 32]>, _: Target) -> Result<Option<Asset>, Error> {
        self.step("asset")?;
        Ok((!self.empty).then_some(Asset {
            id: id(),
            bytes: 1,
            digest: [1; 32],
        }))
    }
    async fn branding(&self, _: [u8; 32]) -> Result<Branding, Error> {
        unreachable!()
    }
    async fn garbage(&self) -> Result<Vec<AssetId>, Error> {
        self.step("garbage")?;
        Ok(vec![id()])
    }
    async fn cleaned(&self, _: AssetId) -> Result<(), Error> {
        self.step("cleaned")
    }
}
impl Images for Fake {
    async fn prepare(&self, _: Kind, _: String, _: Vec<u8>) -> Result<Prepared, Error> {
        self.step("decode")?;
        Ok(Prepared {
            bytes: vec![1],
            digest: [1; 32],
            width: 1,
            height: 1,
        })
    }
}
impl Objects for Fake {
    async fn put(&self, _: AssetId, _: &[u8]) -> Result<(), Error> {
        self.step("put")
    }
    async fn get(&self, _: &Asset) -> Result<Vec<u8>, Error> {
        self.step("get")?;
        Ok(vec![1])
    }
    async fn delete(&self, _: AssetId) -> Result<(), Error> {
        self.step("delete")
    }
}
fn run<T>(future: impl Future<Output = T>) -> T {
    let mut f = std::pin::pin!(future);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fake yielded"),
    }
}
#[test]
fn uploads_persist_intent_before_image_work_and_stop_at_every_failure() {
    for fail in [
        None,
        Some("reserve"),
        Some("decode"),
        Some("put"),
        Some("attach"),
    ] {
        let fake = Fake {
            calls: Default::default(),
            fail,
            empty: false,
        };
        let service = Service {
            store: fake.clone(),
            objects: fake.clone(),
            images: fake.clone(),
        };
        let result = run(service.upload([1; 32], target(), 0, "image/png".into(), vec![1]));
        assert_eq!(result.is_ok(), fail.is_none());
        let order = ["reserve", "decode", "put", "attach"];
        let len = fail
            .map(|f| order.iter().position(|s| *s == f).unwrap() + 1)
            .unwrap_or(4);
        assert_eq!(*fake.calls.lock().unwrap(), order[..len]);
    }
}
#[test]
fn reads_check_authority_before_cloud_io_and_cleanup_retains_failed_work() {
    for fail in [
        None,
        Some("asset"),
        Some("get"),
        Some("garbage"),
        Some("delete"),
        Some("cleaned"),
    ] {
        let fake = Fake {
            calls: Default::default(),
            fail,
            empty: false,
        };
        let service = Service {
            store: fake.clone(),
            objects: fake.clone(),
            images: fake.clone(),
        };
        assert_eq!(
            run(service.read(Some([1; 32]), target())).is_ok(),
            !matches!(fail, Some("asset" | "get"))
        );
        assert_eq!(
            run(service.sweep()).is_ok(),
            !matches!(fail, Some("garbage" | "delete" | "cleaned"))
        );
    }
    let fake = Fake {
        calls: Default::default(),
        fail: None,
        empty: true,
    };
    let service = Service {
        store: fake.clone(),
        objects: fake.clone(),
        images: fake.clone(),
    };
    assert_eq!(run(service.read(None, target())).unwrap(), None);
    assert_eq!(*fake.calls.lock().unwrap(), vec!["asset"]);
    assert_eq!(
        run(service.upload([1; 32], target(), 0, "image/png".into(), vec![])),
        Err(Error::Invalid)
    );
}
