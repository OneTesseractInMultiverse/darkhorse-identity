use super::output::Failure;
use nix::sys::signal::Signal;
use std::future::Future;
use tokio::signal::unix::{SignalKind, signal};

// Installed only for interactive bootstrap and account commands. Keep listeners through its entire
// command because Tokio does not restore default dispositions when they drop.
pub(super) async fn run<T>(work: impl Future<Output = Result<T, Failure>>) -> Result<T, Failure> {
    let mut signals = [
        Signal::SIGINT,
        Signal::SIGTERM,
        Signal::SIGHUP,
        Signal::SIGQUIT,
        Signal::SIGTSTP,
        Signal::SIGTTIN,
        Signal::SIGTTOU,
    ]
    .into_iter()
    .map(|kind| signal(SignalKind::from_raw(kind as i32)))
    .collect::<Result<Vec<_>, _>>()
    .map_err(|_| "Cannot prepare terminal cancellation.")?;
    race(work, async {
        let pending = signals
            .iter_mut()
            .map(|signal| Box::pin(signal.recv()))
            .collect::<Vec<_>>();
        futures_util::future::select_all(pending).await;
    })
    .await
}
async fn race<T>(
    work: impl Future<Output = Result<T, Failure>>,
    cancel: impl Future<Output = ()>,
) -> Result<T, Failure> {
    tokio::select! {
        biased;
        () = cancel => Err(Failure::interrupted()),
        result = work => result,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/operator/cancellation.rs"]
mod tests;
