use super::*;

#[test]
fn durations_and_outcomes_have_bounded_non_cumulative_buckets() {
    let mut sample = Histogram::EMPTY;
    sample.record(0, Outcome::Ok);
    sample.record(5, Outcome::Error);
    sample.record(6, Outcome::Cancelled);
    sample.record(UPPER_US[UPPER_US.len() - 1] + 1, Outcome::Ok);
    assert_eq!((sample.ok, sample.error, sample.cancelled), (2, 1, 1));
    assert_eq!(sample.buckets[0], 1);
    assert_eq!(sample.buckets[1], 1);
    assert_eq!(sample.buckets[2], 1);
    assert_eq!(sample.buckets[UPPER_US.len()], 1);
    assert_eq!(sample.buckets.iter().sum::<u64>(), 4);
    assert_eq!(sample.sum_us, 10_000_012);
    assert_eq!(sample.max_us, 10_000_001);
}

#[test]
fn counters_saturate_instead_of_panicking_or_wrapping() {
    let mut sample = Histogram {
        ok: u64::MAX,
        sum_us: u64::MAX,
        ..Histogram::EMPTY
    };
    sample.buckets[0] = u64::MAX;
    sample.record(1, Outcome::Ok);
    assert_eq!(sample.ok, u64::MAX);
    assert_eq!(sample.sum_us, u64::MAX);
    assert_eq!(sample.buckets[0], u64::MAX);
}
