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
