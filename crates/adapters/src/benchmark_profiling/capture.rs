use super::{
    Stage,
    histogram::{Histogram, Outcome, UPPER_US},
};
use std::{io::Write, sync::Mutex, time::Instant};

const STAGES: [&str; super::STAGE_COUNT] = [
    "service_slot",
    "service_handler",
    "admission_global",
    "admission_global_queue",
    "admission_global_counter",
    "admission_authenticated",
    "admission_caller_authentication",
    "admission_caller_counter",
    "limiter_slot",
    "limiter_operation",
    "total",
    "pool_acquire",
    "begin",
    "fence",
    "authenticate",
    "inspect",
    "policy_load",
    "decision",
    "commit",
    "client_total",
    "client_pool_acquire",
    "client_begin",
    "client_fence",
    "client_authenticate",
    "client_inspect",
    "client_commit",
];
static COUNTERS: Mutex<[Histogram; super::STAGE_COUNT]> =
    Mutex::new([Histogram::EMPTY; super::STAGE_COUNT]);

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
fn take() -> [Histogram; super::STAGE_COUNT] {
    let mut counters = COUNTERS.lock().unwrap_or_else(|poison| poison.into_inner());
    std::mem::replace(&mut *counters, [Histogram::EMPTY; super::STAGE_COUNT])
}
fn encode(counters: [Histogram; super::STAGE_COUNT]) -> serde_json::Value {
    let stages: serde_json::Map<_, _> = STAGES
        .into_iter()
        .zip(counters)
        .map(|(name, histogram)| (name.into(), serde_json::json!(histogram)))
        .collect();
    serde_json::json!({ "schema": 2, "upper_us": UPPER_US, "stages": stages })
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

#[cfg(test)]
#[path = "../../tests/unit/benchmark_profiling/capture.rs"]
mod tests;
