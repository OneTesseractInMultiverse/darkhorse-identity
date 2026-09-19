//! Fixed-cardinality benchmark instrumentation; no identity or query values.
use std::future::Future;

#[derive(Clone, Copy)]
pub(crate) enum Stage {
    Total,
    PoolAcquire,
    Begin,
    Fence,
    Authenticate,
    Inspect,
    PolicyLoad,
    Decision,
    Commit,
}

pub(crate) async fn measure<T, E>(
    stage: Stage,
    future: impl Future<Output = Result<T, E>>,
) -> Result<T, E> {
    #[cfg(feature = "benchmark-profiling")]
    let timer = capture::Timer::start(stage);
    let _ = stage;
    let result = future.await;
    #[cfg(feature = "benchmark-profiling")]
    timer.complete(result.is_ok());
    result
}
pub(crate) fn compute<T, E>(
    stage: Stage,
    operation: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    #[cfg(feature = "benchmark-profiling")]
    let timer = capture::Timer::start(stage);
    let _ = stage;
    let result = operation();
    #[cfg(feature = "benchmark-profiling")]
    timer.complete(result.is_ok());
    result
}
#[cfg(feature = "benchmark-profiling")]
mod capture;
#[cfg(any(test, feature = "benchmark-profiling"))]
mod histogram;
#[cfg(feature = "benchmark-profiling")]
pub use capture::install_signal_reporter;
