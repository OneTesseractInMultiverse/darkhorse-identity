use serde::Serialize;

pub(super) const UPPER_US: [u64; 21] = [
    1, 5, 10, 25, 50, 100, 250, 500, 1_000, 2_500, 5_000, 10_000, 25_000, 50_000, 100_000, 250_000,
    500_000, 1_000_000, 2_500_000, 5_000_000, 10_000_000,
];
#[derive(Clone, Copy)]
pub(super) enum Outcome {
    Ok,
    Error,
    Cancelled,
}
#[derive(Clone, Copy, Serialize)]
pub(super) struct Histogram {
    pub ok: u64,
    pub error: u64,
    pub cancelled: u64,
    pub sum_us: u64,
    pub max_us: u64,
    pub buckets: [u64; 22],
}
impl Histogram {
    pub const EMPTY: Self = Self {
        ok: 0,
        error: 0,
        cancelled: 0,
        sum_us: 0,
        max_us: 0,
        buckets: [0; 22],
    };
    pub fn record(&mut self, elapsed_us: u64, outcome: Outcome) {
        let count = match outcome {
            Outcome::Ok => &mut self.ok,
            Outcome::Error => &mut self.error,
            Outcome::Cancelled => &mut self.cancelled,
        };
        *count = count.saturating_add(1);
        self.sum_us = self.sum_us.saturating_add(elapsed_us);
        self.max_us = self.max_us.max(elapsed_us);
        let index = UPPER_US.partition_point(|upper| *upper < elapsed_us);
        self.buckets[index] = self.buckets[index].saturating_add(1);
    }
}
#[cfg(test)]
#[path = "../../../tests/unit/postgres/profiling/histogram.rs"]
mod tests;
