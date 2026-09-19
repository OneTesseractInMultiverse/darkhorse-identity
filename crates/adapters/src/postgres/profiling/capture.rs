use super::{
    Stage,
    histogram::{Histogram, Outcome, UPPER_US},
};
use std::{io::Write, sync::Mutex, time::Instant};

const STAGES: [&str; 9] = [
    "total",
    "pool_acquire",
    "begin",
    "fence",
    "authenticate",
    "inspect",
    "policy_load",
    "decision",
    "commit",
];
static COUNTERS: Mutex<[Histogram; 9]> = Mutex::new([Histogram::EMPTY; 9]);

pub(super) struct Timer {
    stage: Stage,
    start: Instant,
    outcome: Outcome,
}
impl Timer {
    pub fn start(stage: Stage) -> Self {
        Self {
            stage,
            start: Instant::now(),
            outcome: Outcome::Cancelled,
        }
    }
    pub fn complete(mut self, success: bool) {
        self.outcome = if success { Outcome::Ok } else { Outcome::Error };
    }
}
impl Drop for Timer {
    fn drop(&mut self) {
        let elapsed_us = u64::try_from(self.start.elapsed().as_micros()).unwrap_or(u64::MAX);
        let mut counters = COUNTERS.lock().unwrap_or_else(|poison| poison.into_inner());
        counters[self.stage as usize].record(elapsed_us, self.outcome);
    }
}
fn take() -> [Histogram; 9] {
    let mut counters = COUNTERS.lock().unwrap_or_else(|poison| poison.into_inner());
    std::mem::replace(&mut *counters, [Histogram::EMPTY; 9])
}
fn encode(counters: [Histogram; 9]) -> serde_json::Value {
    let stages: serde_json::Map<_, _> = STAGES
        .into_iter()
        .zip(counters)
        .map(|(name, histogram)| (name.into(), serde_json::json!(histogram)))
        .collect();
    serde_json::json!({ "schema": 1, "upper_us": UPPER_US, "stages": stages })
}
/// Benchmark builds only. The owning harness requests and resets counters at
/// quiescent phase boundaries through SIGUSR1; no network route is installed.
pub fn install_signal_reporter() -> std::io::Result<tokio::task::JoinHandle<()>> {
    let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::user_defined1())?;
    Ok(tokio::spawn(async move {
        while signal.recv().await.is_some() {
            let report = encode(take());
            let _ = writeln!(std::io::stdout().lock(), "DARKHORSE_PROFILE {report}");
        }
    }))
}
