use super::*;
#[test]
fn resource_secrets_have_a_distinct_deterministic_verifier() {
    let secret = from_bytes([0xab; 32], CredentialId::from_u128(7).unwrap());
    assert_eq!(secret.value, "ab".repeat(32));
    assert_eq!(
        secret.verifier.digest,
        secret_digest(&secret.value).unwrap()
    );
    assert_ne!(
        secret.verifier.digest,
        crate::registration::secret_digest(&secret.value).unwrap()
    );
    assert_ne!(
        secret.verifier.digest,
        crate::session_secret::digest(&secret.value).unwrap()
    );
    for invalid in ["", "hello", &"AB".repeat(32), &"ab".repeat(31)] {
        assert!(secret_digest(invalid).is_err());
    }
}
