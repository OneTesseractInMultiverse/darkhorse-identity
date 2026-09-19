use super::*;
#[test]
fn refresh_verifiers_are_distinct_from_access_and_authorization_codes() {
    let text = format!("dr_{}", "ab".repeat(32));
    let refresh = digest(&text, Purpose::Refresh).unwrap();
    assert!(digest(&text, Purpose::Access).is_err());
    assert!(digest(&text, Purpose::Code).is_err());
    assert_ne!(
        refresh,
        digest(&text.replace("dr_", "da_"), Purpose::Access).unwrap()
    );
    assert!(digest(&text.replace("dr_", "da_"), Purpose::Refresh).is_err());
}
#[test]
fn credentials_have_distinct_purposes_and_jwt_inputs_never_match() {
    let code = generate(Purpose::Code).unwrap();
    let access = generate(Purpose::Access).unwrap();
    assert_eq!(digest(&code.value, Purpose::Code), Ok(code.digest));
    assert_eq!(digest(&access.value, Purpose::Access), Ok(access.digest));
    assert!(digest(&code.value, Purpose::Access).is_err());
    assert!(digest("eyJhbGciOiJSUzI1NiJ9.payload.signature", Purpose::Access).is_err());
    assert_ne!(code.digest, access.digest);
    assert!(challenge(&"a".repeat(42)).is_err());
    assert!(challenge(&"a".repeat(129)).is_err());
    assert!(challenge(&"!".repeat(43)).is_err());
    assert!(challenge(&"a".repeat(43)).is_ok());
}
