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
