use darkhorse_application::email_delivery::DeliveryResult;
use darkhorse_domain::email_delivery as policy;
pub(super) fn outcome(
    result: DeliveryResult,
    attempt: u16,
    now: u64,
    expires: u64,
) -> (&'static str, &'static str, Option<u64>) {
    match result {
        DeliveryResult::Accepted => ("accepted", "accepted", None),
        DeliveryResult::Retry => match policy::retry(attempt, now, expires) {
            Some(next) => ("queued", "retry", Some(next)),
            None => ("failed", "failed", None),
        },
        DeliveryResult::Rejected => ("failed", "failed", None),
    }
}
