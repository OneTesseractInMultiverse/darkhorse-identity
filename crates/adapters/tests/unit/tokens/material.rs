use super::*;
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
