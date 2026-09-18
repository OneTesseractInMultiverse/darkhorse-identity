use super::*;
#[test]
fn handles_are_fixed_width_verifier_only_and_canonical() {
    let secret = from_bytes([0xab; 32]);
    assert_eq!(secret.value, "ab".repeat(32));
    assert_eq!(digest(&secret.value), Ok(secret.digest));
    assert_ne!(secret.digest, [0xab; 32]);
    assert_ne!(secret.digest, from_bytes([0xac; 32]).digest);
    for bad in [
        "".into(),
        "0".repeat(63),
        "0".repeat(65),
        "A".repeat(64),
        "g".repeat(64),
        "é".repeat(32),
    ] {
        assert!(digest(&bad).is_err());
    }
}
