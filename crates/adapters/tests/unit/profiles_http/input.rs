use super::*;
fn body() -> serde_json::Value {
    json!({"revision":"0","first_name":"Ana","second_name":"","last_name":"Guzmán","second_last_name":"","country":"CR","calling_code":"506","national_number":"88887777","bio":"Hello"})
}
#[test]
fn strict_profile_inputs_cannot_set_security_state_or_unbounded_revisions() {
    let decoded: Input = serde_json::from_value(body()).unwrap();
    assert!(prepare(decoded).is_ok());
    for field in [
        "email",
        "active",
        "administrator",
        "email_verified",
        "roles",
        "extensions",
        "picture",
    ] {
        let mut b = body();
        b[field] = json!(true);
        assert!(serde_json::from_value::<Input>(b).is_err());
    }
    for revision in ["", "01", "-1", "9223372036854775808"] {
        let mut b = body();
        b["revision"] = json!(revision);
        assert!(prepare(serde_json::from_value(b).unwrap()).is_err());
    }
    for (code, number) in [("", "88887777"), ("506", ""), ("999", "88887777")] {
        let mut b = body();
        b["calling_code"] = json!(code);
        b["national_number"] = json!(number);
        assert!(prepare(serde_json::from_value(b).unwrap()).is_err());
    }
    let mut b = body();
    b["calling_code"] = json!("");
    b["national_number"] = json!("");
    assert!(prepare(serde_json::from_value(b).unwrap()).is_ok());
    assert_eq!(target("me"), Ok(None));
    assert_eq!(
        target("00000000-0000-0000-0000-000000000002")
            .unwrap()
            .unwrap()
            .as_u128(),
        2
    );
    for bad in [
        "ME",
        "00000000-0000-0000-0000-000000000000",
        "00000000000000000000000000000002",
        "nobody",
    ] {
        assert_eq!(target(bad), Err(Error::Invalid));
    }
}
