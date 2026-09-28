//! Fixed-cardinality benchmark instrumentation; no identity or query values.
use std::future::Future;

#[derive(Clone, Copy)]
#[repr(usize)]
pub(crate) enum Stage {
    ServiceSlot,
    ServiceHandler,
    AdmissionGlobal,
    AdmissionGlobalQueue,
    AdmissionGlobalCounter,
    AdmissionAuthenticated,
    AdmissionCallerAuthentication,
    AdmissionCallerCounter,
    LimiterSlot,
    LimiterOperation,
    Total,
    PoolAcquire,
    Begin,
    Fence,
    Authenticate,
    Inspect,
    PolicyLoad,
    Decision,
    Commit,
    ClientTotal,
    ClientPoolAcquire,
    ClientBegin,
    ClientFence,
    ClientAuthenticate,
    ClientInspect,
    ClientCommit,
}
#[cfg(feature = "benchmark-profiling")]
pub(crate) const STAGE_COUNT: usize = Stage::ClientCommit as usize + 1;

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
