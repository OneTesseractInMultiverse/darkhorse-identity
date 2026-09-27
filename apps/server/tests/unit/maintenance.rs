use super::*;
use darkhorse_application::oidc_maintenance::AuthorizationRequestCleanupSweep;

#[test]
fn empty_cleanup_sweep_does_not_emit_an_event() {
    assert_eq!(
        authorization_cleanup_event(AuthorizationRequestCleanupSweep {
            batches: 1,
            deleted: 0,
            oldest_expired_age_ms: None,
            backlog_remaining: false,
        }),
        None
    );
}

#[test]
fn cleanup_event_uses_stable_fields_and_contains_no_request_identifiers() {
    assert_eq!(
        authorization_cleanup_event(AuthorizationRequestCleanupSweep {
            batches: 10,
            deleted: 1_000,
            oldest_expired_age_ms: Some(1_800_000),
            backlog_remaining: true,
        }),
        Some("maintenance authorization_request_cleanup status=ok batches=10 deleted=1000 backlog_remaining=true oldest_expired_age_ms=1800000".to_owned())
    );
    assert_eq!(
        authorization_cleanup_event(AuthorizationRequestCleanupSweep {
            batches: 1,
            deleted: 10,
            oldest_expired_age_ms: None,
            backlog_remaining: false,
        }),
        Some("maintenance authorization_request_cleanup status=ok batches=1 deleted=10 backlog_remaining=false oldest_expired_age_ms=none".to_owned())
    );
}
