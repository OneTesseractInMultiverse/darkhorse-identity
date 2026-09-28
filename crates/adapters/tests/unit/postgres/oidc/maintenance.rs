use super::*;

#[test]
fn cleanup_row_projection_reports_counts_age_and_remaining_backlog() {
    assert_eq!(
        cleanup_batch(100, Some(400), Some(900)),
        Ok(AuthorizationRequestCleanupBatch {
            deleted: 100,
            oldest_expired_age_ms: Some(900),
            backlog_remaining: true,
        })
    );
    assert_eq!(
        cleanup_batch(12, Some(900), None),
        Ok(AuthorizationRequestCleanupBatch {
            deleted: 12,
            oldest_expired_age_ms: Some(900),
            backlog_remaining: false,
        })
    );
    assert_eq!(
        cleanup_batch(0, None, None),
        Ok(AuthorizationRequestCleanupBatch {
            deleted: 0,
            oldest_expired_age_ms: None,
            backlog_remaining: false,
        })
    );
}

#[test]
fn invalid_database_counts_and_ages_fail_closed() {
    assert_eq!(cleanup_batch(-1, None, None), Err(Error::Unavailable));
    assert_eq!(cleanup_batch(0, Some(-1), None), Err(Error::Unavailable));
    assert_eq!(cleanup_batch(0, None, Some(-1)), Err(Error::Unavailable));
}

#[test]
fn credential_cleanup_sql_preserves_refresh_children_and_requires_no_code_dependents() {
    let access = credential_cleanup_sql(CredentialRecordCategory::LegacyAccessTokens);
    assert!(access.contains("refresh_generation IS NULL"));
    assert!(access.contains("expires_ms<=$1"));
    assert!(access.contains("FOR UPDATE SKIP LOCKED"));
    let codes = credential_cleanup_sql(CredentialRecordCategory::AuthorizationCodes);
    assert!(codes.contains("NOT EXISTS(SELECT 1 FROM access_tokens"));
    assert!(codes.contains("NOT EXISTS(SELECT 1 FROM refresh_families"));
    assert!(codes.contains("FOR UPDATE SKIP LOCKED"));
}

#[test]
fn credential_cleanup_projection_rejects_negative_database_facts() {
    assert_eq!(
        credential_cleanup_batch(-1, None, None),
        Err(Error::Unavailable)
    );
    assert_eq!(
        credential_cleanup_batch(0, Some(-1), None),
        Err(Error::Unavailable)
    );
    assert_eq!(
        credential_cleanup_batch(0, None, Some(-1)),
        Err(Error::Unavailable)
    );
    assert_eq!(
        credential_cleanup_batch(40, Some(20), Some(90)),
        Ok(CredentialRecordCleanupBatch {
            deleted: 40,
            oldest_expired_age_ms: Some(90),
            backlog_remaining: true,
        })
    );
}
