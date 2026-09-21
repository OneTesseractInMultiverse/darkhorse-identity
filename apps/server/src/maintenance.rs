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

/// One in-flight SMTP operation per process; no network operation holds a SQL transaction.
pub async fn email(
    runtime: Option<(
        PostgresStore,
        darkhorse_adapters::email_verification::smtp::Smtp,
    )>,
) {
    let Some((store, sender)) = runtime else {
        return std::future::pending().await;
    };
    loop {
        let result = darkhorse_application::email_verification::deliver_next(&store, &sender).await;
        let invitation = darkhorse_application::invitations::deliver_next(&store, &sender).await;
        if result.is_err() || invitation.is_err() {
            eprintln!("Email queue unavailable; retrying after a delay.");
        }
        if !matches!(result, Ok(true)) && !matches!(invitation, Ok(true)) {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
}

/// Tombstones outlive network attempts and are revisited for delayed remote writes.
pub async fn media(runtime: Option<crate::authentication::Media>) {
    let Some(service) = runtime else {
        return std::future::pending().await;
    };
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        if service.sweep().await.is_err() {
            eprintln!("Media cleanup unavailable; retrying next interval.");
        }
    }
}
