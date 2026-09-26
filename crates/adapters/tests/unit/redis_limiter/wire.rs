use super::*;
#[test]
fn counter_wire_is_bounded_canonical_and_preserves_large_exact_times() {
    let c = Counter {
        rule: BudgetRule::new(1, 1000).unwrap(),
        used: 1,
        started_ms: 9_007_199_254_738_991,
        last_ms: 9_007_199_254_739_991,
    };
    assert_eq!(decode(&encode(Some(c))), Ok(Some(c)));
    assert_eq!(decode(&encode(None)), Ok(None));
    for text in [
        "1",
        "1,1,1,0,0",
        "0,1000,1,0,0",
        "01,1000,1,0,0",
        "1,1000,x,0,0",
        "1,1000,1,-1,0",
        "1,1000,1,0,18446744073709551616",
        &"1".repeat(97),
    ] {
        assert!(decode(text).is_err(), "{text}");
    }
    assert_eq!(unhex::<2>(&hex(&[0, 255])), Ok([0, 255]));
    for text in ["00", "00FF", "00gg", "é00"] {
        assert!(unhex::<2>(text).is_err());
    }
}

#[test]
fn counter_policy_binding_is_canonical_and_legacy_counters_keep_their_format() {
    let legacy = Counter {
        rule: BudgetRule::new(100, 60000).unwrap(),
        used: 1,
        started_ms: 100,
        last_ms: 101,
    };
    assert_eq!(encode(Some(legacy)), "100,60000,1,100,101");
    let bound = Counter {
        rule: legacy.rule.bound_to(10),
        ..legacy
    };
    assert_eq!(encode(Some(bound)), "100,60000,1,100,101,10");
    assert_eq!(decode(&encode(Some(bound))), Ok(Some(bound)));
    for suffix in ["0", "01", "-1", "4294967296", "10,20"] {
        assert!(decode(&format!("100,60000,1,100,101,{suffix}")).is_err());
    }
}
