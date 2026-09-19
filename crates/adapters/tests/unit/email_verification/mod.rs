use super::*;
#[test]
fn verification_tokens_are_canonical_purpose_bound_and_key_bound() {
    for key in ["", &"0".repeat(64), &"A".repeat(64)] {
        assert!(Secrets::from_hex(key).is_err());
    }
    let key = Secrets::from_hex(&"ab".repeat(32)).unwrap();
    let other = Secrets::from_hex(&"cd".repeat(32)).unwrap();
    let token = key.token([3; 32]);
    assert_eq!(token.len(), 68);
    assert!(token.starts_with("ev1_"));
    assert_ne!(token, key.token([4; 32]));
    assert_ne!(token, other.token([3; 32]));
    assert_ne!(key.fingerprint(), other.fingerprint());
    assert!(token_digest(&token).is_ok());
    for invalid in [
        token[4..].to_owned(),
        token.to_uppercase(),
        format!("{} ", token.as_str()),
        "ev1_bad".into(),
    ] {
        assert_eq!(token_digest(&invalid), Err(Error::Invalid));
    }
}
#[test]
fn invitation_proofs_are_independent_from_email_verification_and_canonical() {
    use darkhorse_application::invitations::InvitationSecrets;
    let secrets = Secrets::from_hex(&"ab".repeat(32)).unwrap();
    let token = secrets.invitation_token([7; 32]);
    assert_ne!(&token[4..], &secrets.token([7; 32])[4..]);
    assert!(token_digest(&token).is_err());
    assert!(invitation_digest(&secrets.token([7; 32])).is_err());
    assert!(invitation_digest(&token).is_ok());
    for malformed in [
        "iv1_bad".to_owned(),
        token.to_uppercase(),
        format!("{}\n", token.as_str()),
    ] {
        assert!(invitation_digest(&malformed).is_err());
    }
    let a = secrets.issue_invitation().unwrap();
    let b = secrets.issue_invitation().unwrap();
    assert_ne!(a.id, b.id);
    assert_ne!(a.seed, b.seed);
    assert_eq!(
        a.digest,
        invitation_digest(&secrets.invitation_token(a.seed)).unwrap()
    );
}
