use super::*;
fn id(n: u128) -> String {
    uuid::Uuid::from_u128(n).to_string()
}
fn input() -> serde_json::Value {
    serde_json::json!({"name":"worker","application_id":id(1),"policy_revision":"9","expiration":{"kind":"default"},"grants":[{"resource_id":id(2),"selection":{"kind":"all"}}]})
}
#[test]
fn canonical_resource_selection_and_expiration_are_strict() {
    let parsed = request(serde_json::from_value(input()).unwrap()).unwrap();
    assert_eq!(parsed.revision(), 9);
    assert_eq!(parsed.expiration(), Expiration::Default);
    for expiry in [
        serde_json::json!({"kind":"never"}),
        serde_json::json!({"kind":"days","days":30}),
    ] {
        let mut value = input();
        value["expiration"] = expiry;
        assert!(request(serde_json::from_value(value).unwrap()).is_ok());
    }
    let mut subset = input();
    subset["grants"][0]["selection"] = serde_json::json!({"kind":"subset","capabilities":[id(3)]});
    assert!(request(serde_json::from_value(subset.clone()).unwrap()).is_ok());
    subset["grants"][0]["selection"]["capabilities"] = serde_json::json!([id(3), id(3)]);
    assert!(request(serde_json::from_value(subset).unwrap()).is_err());
    for field in ["application_id", "policy_revision"] {
        for bad in ["", "01", "18446744073709551616"] {
            let mut value = input();
            value[field] = bad.into();
            assert!(request(serde_json::from_value(value).unwrap()).is_err());
        }
    }
    for bad in [
        id(0),
        "invalid".into(),
        "AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA".into(),
    ] {
        assert!(identifier(&bad).is_err());
    }
    for extra in ["secret", "principal_id", "scopes"] {
        let mut value = input();
        value[extra] = "unexpected".into();
        assert!(serde_json::from_value::<Creation>(value).is_err());
    }
    let mut value = input();
    value["grants"][0]["resource_id"] = "bad".into();
    assert!(request(serde_json::from_value(value).unwrap()).is_err());
    let mut value = input();
    value["grants"][0]["selection"] = serde_json::json!({"kind":"subset","capabilities":["bad"]});
    assert!(request(serde_json::from_value(value).unwrap()).is_err());
}
#[test]
fn cursor_allows_only_one_canonical_nonzero_reference() {
    assert_eq!(cursor(None), Ok(None));
    assert_eq!(cursor(Some(&format!("after={}", id(2)))), Ok(Some(2)));
    for query in [
        "",
        "skip=1",
        "after=bad",
        &format!("after={}&after={}", id(1), id(2)),
        &"a".repeat(129),
    ] {
        assert_eq!(cursor(Some(query)), Err(Error::Invalid));
    }
}
