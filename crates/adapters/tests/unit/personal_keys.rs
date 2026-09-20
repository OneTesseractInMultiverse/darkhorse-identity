use super::*;
#[test]
fn personal_secret_format_and_verification_are_distinct_from_other_credentials() {
    let prepared = prepare([1; 16], &[2; 32]).unwrap();
    assert_eq!(prepared.value, format!("dk_{}", "02".repeat(32)));
    assert_eq!(digest(&prepared.value).unwrap(), prepared.verifier.digest);
    assert_eq!(prepared.verifier.id.as_u128(), u128::from_be_bytes([1; 16]));
    assert!(prepare([0; 16], &[1; 32]).is_err());
    for value in [
        "",
        "dk_",
        &format!("dk_{}", "A".repeat(64)),
        &format!("da_{}", "02".repeat(32)),
        &format!("dk_{}", "f".repeat(65)),
        &format!("dk_{}!", "f".repeat(63)),
    ] {
        assert!(digest(value).is_err());
    }
    assert_ne!(
        prepared.verifier.digest,
        Sha256::digest(prepared.value.as_bytes()).as_slice()
    );
}
