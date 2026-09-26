use super::*;
use darkhorse_domain::identity::{ClientId, ResourceId};
#[test]
fn identities_are_stable_purpose_bound_and_independent_of_policy_or_secrets() {
    let p = Policy::new(100, 10).unwrap();
    let client = Caller::Client(ClientId::from_u128(1).unwrap());
    let resource = Caller::Resource(ResourceId::from_u128(1).unwrap());
    let a = attempt(&[7; 32], p, Some(client)).unwrap();
    assert_eq!(a, attempt(&[7; 32], p, Some(client)).unwrap());
    for other in [
        None,
        Some(resource),
        Some(Caller::Client(ClientId::from_u128(2).unwrap())),
    ] {
        assert_ne!(
            a.budgets()[0].key,
            attempt(&[7; 32], p, other).unwrap().budgets()[0].key
        );
    }
    assert_ne!(
        a.budgets()[0].key,
        attempt(&[8; 32], p, Some(client)).unwrap().budgets()[0].key
    );
    let changed = attempt(&[7; 32], Policy::new(200, 20).unwrap(), Some(client)).unwrap();
    assert_eq!(a.budgets()[0].key, changed.budgets()[0].key);
    let global = attempt(&[7; 32], p, None).unwrap();
    let caller_changed = attempt(&[7; 32], Policy::new(100, 20).unwrap(), None).unwrap();
    assert_eq!(global.budgets()[0].key, caller_changed.budgets()[0].key);
    assert_ne!(global.budgets()[0].rule, caller_changed.budgets()[0].rule);
    assert_eq!(a.budgets()[0].rule.limit(), 10);
    assert_eq!(a.budgets()[0].rule.window_ms(), 60_000);
    assert_eq!(
        attempt(&[7; 32], p, None).unwrap().budgets()[0]
            .rule
            .limit(),
        100
    );
    assert_eq!(outcome(Admission::Allowed), Ok(()));
    assert_eq!(
        outcome(Admission::Limited {
            retry_after_ms: 1001
        }),
        Err(Error::Limited {
            retry_after_ms: 1001
        })
    );
}
#[test]
fn configuration_is_bounded_consistent_and_cannot_disable_enforcement() {
    use envbind::MapEnvironment;
    assert_eq!(
        load(MapEnvironment::new()).unwrap(),
        Policy::new(60_000, 6_000).unwrap()
    );
    for (global, caller) in [
        ("0", "1"),
        ("-1", "1"),
        ("4294967296", "1"),
        ("10", "11"),
        ("1", "0"),
        ("1000001", "1"),
        ("secret", "1"),
        ("10", "-1"),
    ] {
        assert_eq!(
            load(MapEnvironment::from_pairs([
                ("DARKHORSE_INTROSPECTION_GLOBAL_PER_MINUTE", global),
                ("DARKHORSE_INTROSPECTION_CALLER_PER_MINUTE", caller),
            ]))
            .err(),
            Some(ConfigurationError)
        );
    }
    assert!(Policy::new(1, 1).is_ok());
    assert!(Policy::new(1_000_000, 1_000_000).is_ok());
}

#[tokio::test]
async fn global_queue_bounds_waiters_and_releases_capacity_on_cancellation() {
    let queue = GlobalQueue::default();
    let mut permits = Vec::new();
    for _ in 0..16 {
        permits.push(queue.enter().unwrap());
    }
    assert!(matches!(queue.enter(), Err(Error::Unavailable)));
    permits.pop();
    assert!(queue.enter().is_ok());
    drop(permits);
    assert_eq!(queue.waiters.available_permits(), 16);
    // Cancellation of a waiter cannot retain either permit or an execution lane.
    let held = queue.lanes.acquire_many(2).await.unwrap();
    {
        let mut waiting = std::pin::pin!(queue.lock());
        use std::future::Future;
        use std::task::{Context, Poll, Waker};
        assert!(matches!(
            waiting
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
    }
    assert_eq!(queue.waiters.available_permits(), 16);
    drop(held);
    assert!(queue.lanes.try_acquire().is_ok());
}
