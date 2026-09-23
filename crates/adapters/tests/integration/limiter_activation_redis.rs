use super::*;
use darkhorse_application::limiter_activation::{self, Error, Initializer, Journal};
use darkhorse_domain::{identity::OperationId, limiter_recovery::ServerIdentity};
fn operation() -> OperationId {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).unwrap();
    OperationId::from_u128(
        uuid::Builder::from_random_bytes(bytes)
            .into_uuid()
            .as_u128(),
    )
    .unwrap()
}
#[tokio::test]
async fn failed_redis_initialization_leaves_an_inspectable_pending_intent() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.fence().await;
    f.expire_wait().await;
    let id = operation();
    // Runtime ACL cannot perform recovery initialization.
    assert_eq!(
        limiter_activation::activate(&f.store, &f.counters, id).await,
        Err(Error::Uncertain)
    );
    let pending = f.store.inspect(id).await.unwrap();
    assert!(pending.completion.is_none());
    assert!(!pending.current.active);
    assert_eq!(
        f.limiter().consume(&attempt(25, 2, 10_000)).await,
        Err(LimiterUnavailable)
    );
    let completed = operation();
    limiter_activation::activate(&f.store, &f.operator, completed)
        .await
        .unwrap();
    assert!(
        f.store
            .inspect(completed)
            .await
            .unwrap()
            .completion
            .is_some()
    );
    assert!(f.store.inspect(id).await.unwrap().completion.is_none());
}
struct LostInitialization<'a> {
    counters: &'a RedisCounters,
    calls: std::sync::atomic::AtomicUsize,
}
impl Initializer for LostInitialization<'_> {
    async fn initialize(&self, state: Enforcement) -> Result<ServerIdentity, Error> {
        self.calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.counters.initialize(state).await.unwrap();
        Err(Error::Unavailable)
    }
}
#[tokio::test]
async fn lost_initialization_reply_never_activates_or_retries_and_inspection_has_no_effect() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.fence().await;
    f.expire_wait().await;
    let id = operation();
    let lost = LostInitialization {
        counters: &f.operator,
        calls: std::sync::atomic::AtomicUsize::new(0),
    };
    assert_eq!(
        limiter_activation::activate(&f.store, &lost, id).await,
        Err(Error::Uncertain)
    );
    assert_eq!(lost.calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    for _ in 0..2 {
        let pending = f.store.inspect(id).await.unwrap();
        assert!(pending.completion.is_none());
        assert!(!pending.current.active);
    }
    assert_eq!(
        f.limiter().consume(&attempt(26, 2, 10_000)).await,
        Err(LimiterUnavailable)
    );
    let completed = operation();
    limiter_activation::activate(&f.store, &f.operator, completed)
        .await
        .unwrap();
    let a = attempt(26, 1, 10_000);
    assert_eq!(f.limiter().consume(&a).await, Ok(Admission::Allowed));
    f.store.inspect(completed).await.unwrap();
    assert!(matches!(
        f.limiter().consume(&a).await,
        Ok(Admission::Limited { .. })
    ));
}
