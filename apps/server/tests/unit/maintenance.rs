use super::*;
use darkhorse_application::oidc_maintenance::{
    AuthorizationRequestCleanupSweep, CredentialRecordCategory, CredentialRecordCleanupSweep,
};

#[test]
fn empty_cleanup_sweep_does_not_emit_an_event() {
    assert_eq!(
        authorization_cleanup_event(
            AuthorizationRequestCleanupSweep {
                batches: 1,
                deleted: 0,
                oldest_expired_age_ms: None,
                backlog_remaining: false,
            },
            99
        ),
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
        }, 17),
        Some("maintenance authorization_request_cleanup status=ok batches=10 deleted=1000 backlog_remaining=true oldest_expired_age_ms=1800000 duration_ms=17".to_owned())
    );
    assert_eq!(
        authorization_cleanup_event(AuthorizationRequestCleanupSweep {
            batches: 1,
            deleted: 10,
            oldest_expired_age_ms: None,
            backlog_remaining: false,
        }, 0),
        Some("maintenance authorization_request_cleanup status=ok batches=1 deleted=10 backlog_remaining=false oldest_expired_age_ms=none duration_ms=0".to_owned())
    );
}

#[test]
fn cleanup_failure_event_reports_duration_without_database_details() {
    assert_eq!(
        authorization_cleanup_failure_event(23),
        "maintenance authorization_request_cleanup status=failed error=unavailable duration_ms=23; retrying next interval."
    );
}

#[test]
fn credential_cleanup_events_use_fixed_names_and_aggregate_fields_only() {
    assert_eq!(
        credential_cleanup_event(
            CredentialRecordCategory::LegacyAccessTokens,
            CredentialRecordCleanupSweep {
                batches: 10,
                deleted: 1_000,
                oldest_expired_age_ms: Some(900_000),
                backlog_remaining: true,
            },
            23,
        ),
        Some("maintenance legacy_access_token_cleanup status=ok batches=10 deleted=1000 backlog_remaining=true oldest_expired_age_ms=900000 duration_ms=23".to_owned())
    );
    assert_eq!(
        credential_cleanup_event(
            CredentialRecordCategory::AuthorizationCodes,
            CredentialRecordCleanupSweep {
                batches: 1,
                deleted: 0,
                oldest_expired_age_ms: None,
                backlog_remaining: false,
            },
            4,
        ),
        None
    );
    assert_eq!(
        credential_cleanup_failure_event(CredentialRecordCategory::AuthorizationCodes, 19),
        "maintenance authorization_code_cleanup status=failed error=unavailable duration_ms=19; retrying next interval."
    );
}
