use super::*;

#[test]
fn issuance_limits_are_persistent_and_expiry_is_bounded() {
    assert_eq!(issue(None, 0, 0, 0, 100), Ok(100 + LIFETIME_MS));
    assert_eq!(
        issue(Some(100), 0, 0, 0, 100 + COOLDOWN_MS - 1),
        Err(Error::Throttled)
    );
    assert!(issue(Some(100), 0, 0, 0, 100 + COOLDOWN_MS).is_ok());
    assert_eq!(issue(None, 5, 0, 0, 100), Err(Error::Throttled));
    assert_eq!(issue(None, 0, 100, 0, 100), Err(Error::Throttled));
    assert_eq!(issue(None, 0, 0, 10_000, 100), Err(Error::Unavailable));
    assert_eq!(
        issue(None, 0, 0, 0, i64::MAX as u64),
        Err(Error::Unavailable)
    );
}

#[test]
fn accepting_requires_current_authority_and_a_live_unused_proof() {
    let proof = Proof {
        created_ms: 100,
        expires_ms: 200,
        closed: false,
        issuer_eligible: true,
        account_exists: false,
    };
    assert_eq!(redeem(&proof, 100), Ok(()));
    assert_eq!(redeem(&proof, 199), Ok(()));
    assert_eq!(redeem(&proof, 99), Err(Error::Invalid));
    assert_eq!(redeem(&proof, 200), Err(Error::Invalid));
    assert_eq!(
        redeem(
            &Proof {
                closed: true,
                ..proof
            },
            150
        ),
        Err(Error::Invalid)
    );
    assert_eq!(
        redeem(
            &Proof {
                issuer_eligible: false,
                ..proof
            },
            150
        ),
        Err(Error::Invalid)
    );
    assert_eq!(
        redeem(
            &Proof {
                account_exists: true,
                ..proof
            },
            150
        ),
        Err(Error::Invalid)
    );
}

#[test]
fn hash_limits_do_not_reset_on_failed_work() {
    assert_eq!(admit(4, 59), Ok(()));
    assert_eq!(admit(5, 0), Err(Error::Throttled));
    assert_eq!(admit(0, 60), Err(Error::Throttled));
}
#[test]
fn recipient_uses_existing_directory_email_rules() {
    assert_eq!(email(" New@example.com "), Ok("New@example.com".into()));
    assert_eq!(email("bad"), Err(Error::Invalid));
}

#[test]
fn existing_accounts_are_never_invitation_targets() {
    assert_eq!(ensure_new_recipient(false), Ok(()));
    assert_eq!(ensure_new_recipient(true), Err(Error::Conflict));
}
