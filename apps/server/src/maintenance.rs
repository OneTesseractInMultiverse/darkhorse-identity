use darkhorse_adapters::postgres::PostgresStore;
use darkhorse_application::refresh;
use std::time::Duration;

/// One bounded sweep per minute. Dropping this future cancels pending work;
/// SQL transaction drop rolls it back when the HTTP server finishes shutdown.
pub async fn run(store: Option<PostgresStore>) {
    let Some(store) = store else {
        return std::future::pending().await;
    };
    let period = Duration::from_secs(60);
    let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        if refresh::sweep(&store).await.is_err() {
            eprintln!("Refresh credential cleanup unavailable; retrying next interval.");
        }
    }
}
