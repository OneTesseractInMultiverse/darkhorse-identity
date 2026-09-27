use darkhorse_adapters::postgres::PostgresStore;
use darkhorse_application::{oidc_maintenance, refresh};
use std::time::{Duration, Instant};

#[cfg(test)]
#[path = "../tests/unit/maintenance.rs"]
mod tests;

fn authorization_cleanup_event(
    report: oidc_maintenance::AuthorizationRequestCleanupSweep,
    duration_ms: u64,
) -> Option<String> {
    if report.deleted == 0 && !report.backlog_remaining {
        return None;
    }
    let oldest_age_ms = report
        .oldest_expired_age_ms
        .map_or_else(|| "none".to_owned(), |age| age.to_string());
    Some(format!(
        "maintenance authorization_request_cleanup status=ok batches={} deleted={} backlog_remaining={} oldest_expired_age_ms={oldest_age_ms} duration_ms={duration_ms}",
        report.batches, report.deleted, report.backlog_remaining
    ))
}

fn authorization_cleanup_failure_event(duration_ms: u64) -> String {
    format!(
        "maintenance authorization_request_cleanup status=failed error=unavailable duration_ms={duration_ms}; retrying next interval."
    )
}

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

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
        let started = Instant::now();
        match oidc_maintenance::sweep_expired_authorization_requests(&store).await {
            Ok(report) => {
                if let Some(event) = authorization_cleanup_event(report, elapsed_millis(started)) {
                    eprintln!("{event}");
                }
            }
            Err(_) => eprintln!(
                "{}",
                authorization_cleanup_failure_event(elapsed_millis(started))
            ),
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
