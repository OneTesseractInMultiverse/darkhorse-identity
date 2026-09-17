use super::*;
use crate::redis_configuration;
use darkhorse_domain::{limiter_recovery::Generation, limiting::BudgetRule};
fn state() -> Enforcement {
    Enforcement {
        generation: Generation::new(1, [1; 16]).unwrap(),
        active: true,
        identity: Some(ServerIdentity {
            run: [1; 20],
            replication: [2; 20],
        }),
        now_ms: 3000,
        not_before_ms: 0,
    }
}
#[test]
fn snapshot_and_cleanup_reject_malformed_records_and_keep_live_budgets() {
    assert!(snapshot(vec![], 1).is_err());
    assert!(snapshot(vec!["invalid".into(), "1".into(), "".into()], 1).is_err());
    assert!(snapshot(vec!["1".into(), "invalid".into(), "".into()], 1).is_err());
    assert!(snapshot(vec!["1".into(), "1".into(), "invalid".into()], 1).is_err());
    assert_eq!(
        snapshot(vec!["1".into(), "1".into(), "".into()], 1)
            .unwrap()
            .counters,
        vec![None]
    );
    assert!(valid_until(&[]).is_err());
    let c = Counter {
        rule: BudgetRule::new(1, 1000).unwrap(),
        used: 1,
        started_ms: 2000,
        last_ms: 2000,
    };
    assert!(
        valid_until(&[Counter {
            started_ms: u64::MAX,
            ..c
        }])
        .is_err()
    );
    let key = field(&[1; 32]);
    let pair = vec![key.clone(), wire::encode(Some(c))];
    assert_eq!(
        expired(state(), 3000, 2000, &pair).unwrap(),
        vec![(pair[0].as_str(), pair[1].as_str())]
    );
    assert!(expired(state(), 2999, 2000, &pair).unwrap().is_empty());
    for values in [
        vec![key.clone()],
        vec!["x".into(), "".into()],
        vec!["b:zz".into(), "".into()],
        vec![key.clone(), "".into()],
        vec![key.clone(), "invalid".into()],
        vec![key, "1,1000,0,2000,2000".into()],
    ] {
        assert!(expired(state(), 3000, 2000, &values).is_err());
    }
    assert!(expired(state(), 3000, 3001, &pair).is_err());
    assert!(
        expired(
            state(),
            3000,
            2000,
            &vec![String::new(); 2 * (CAPACITY + 6)]
        )
        .is_err()
    );
    assert!(
        expired(state(), 3000, 2000, &["_clock".into(), "3000".into()])
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn unknown_replies_and_incomplete_proposals_cannot_confirm_a_charge() {
    assert_eq!(completed(1), Ok(()));
    for value in [-1, 0, 2, i64::MAX] {
        assert!(completed(value).is_err());
    }
    for (value, expected) in [
        (0, Commit::Conflict),
        (1, Commit::Applied),
        (2, Commit::Full),
    ] {
        assert_eq!(commit_reply(value), Ok(expected));
    }
    for value in [-1, 3, i64::MAX] {
        assert_eq!(commit_reply(value), Err(LimiterUnavailable));
    }
    let settings = redis_configuration::load(envbind::MapEnvironment::from_pairs([
        (
            "DARKHORSE_REDIS_CACHE_URL",
            "rediss://cache:fixture-one@localhost:63791/0",
        ),
        (
            "DARKHORSE_REDIS_LIMITER_URL",
            "rediss://limiter:fixture-two@localhost:63792/0",
        ),
    ]))
    .unwrap();
    let counters = RedisCounters::new(settings).unwrap();
    let a = Attempt::new(vec![darkhorse_domain::limiting::Budget {
        key: [1; 32],
        rule: BudgetRule::new(1, 1000).unwrap(),
    }])
    .unwrap();
    let snapshot = Snapshot {
        counters: vec![None],
        redis_ms: 3000,
        previous_ms: 3000,
    };
    assert_eq!(
        counters.apply(state(), &a, &snapshot, &[], 3000).await,
        Err(LimiterUnavailable)
    );
}
