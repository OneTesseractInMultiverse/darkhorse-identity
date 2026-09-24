use super::*;
use serde_json::json;
fn value() -> serde_json::Value {
    json!({"name":"Portal","active":true,"refresh_tokens":false,
        "redirect_uris":["https://portal.example/cb?fixed=%2F"],"resource_ids":[],"scope_ids":[],
        "token_endpoint_auth_method":"client_secret_basic"})
}
#[test]
fn shared_client_profile_preserves_exact_callbacks_and_requires_explicit_operator_refresh_policy() {
    let parse = |value| serde_json::from_value::<ClientInput>(value).unwrap();
    let spec = parse(value()).complete_spec().unwrap();
    assert_eq!(
        spec.redirects.values(),
        ["https://portal.example/cb?fixed=%2F"]
    );
    assert!(!spec.refresh_tokens);
    let mut v = value();
    v["refresh_tokens"] = true.into();
    assert!(parse(v).complete_spec().unwrap().refresh_tokens);
    let mut v = value();
    v.as_object_mut().unwrap().remove("refresh_tokens");
    assert!(parse(v.clone()).complete_spec().is_err());
    assert!(!parse(v).spec().unwrap().refresh_tokens);
    for (key, value) in [
        ("refresh_tokens", serde_json::Value::Null),
        ("unknown", true.into()),
    ] {
        let mut v = self::value();
        v[key] = value;
        assert!(serde_json::from_value::<ClientInput>(v).is_err());
    }
}
#[test]
fn shared_client_profile_rejects_unsafe_callbacks_duplicate_allowances_and_other_authentication_methods()
 {
    for (field, value) in [
        ("redirect_uris", json!(["http://portal.example/cb"])),
        (
            "redirect_uris",
            json!(["https://portal.example/cb#fragment"]),
        ),
        (
            "redirect_uris",
            json!(["https://portal.example/cb", "https://portal.example/cb"]),
        ),
        ("redirect_uris", json!([])),
        ("resource_ids", json!(["invalid"])),
        ("scope_ids", json!(["00000000-0000-0000-0000-000000000000"])),
        (
            "scope_ids",
            json!(vec!["00000000-0000-0000-0000-000000000001"; 129]),
        ),
        ("token_endpoint_auth_method", json!("client_secret_jwt")),
    ] {
        let mut v = self::value();
        v[field] = value;
        assert!(
            serde_json::from_value::<ClientInput>(v)
                .unwrap()
                .complete_spec()
                .is_err()
        );
    }
}
