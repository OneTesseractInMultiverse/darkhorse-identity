use super::*;
#[test]
fn counts_outcomes_without_identity_labels() {
    let counters = Counters::default();
    assert_eq!(
        counters.snapshot(),
        Outcomes {
            allowed: 0,
            limited: 0,
            unavailable: 0
        }
    );
    for value in [
        Ok(Admission::Allowed),
        Ok(Admission::Limited { retry_after_ms: 1 }),
        Err(LimiterUnavailable),
        Err(LimiterUnavailable),
    ] {
        counters.record(&value);
    }
    assert_eq!(
        counters.snapshot(),
        Outcomes {
            allowed: 1,
            limited: 1,
            unavailable: 2
        }
    );
}
