use super::*;
use darkhorse_domain::identity::{ClientId, PrincipalId};
#[test]
fn profile_projection_omits_unapproved_fields_and_never_claims_email_verification() {
    let profile = UserInfo {
        subject: PrincipalId::from_u128(1).unwrap(),
        profile: None,
        email: None,
    };
    let result = profile_response(profile);
    assert_eq!(result.as_object().unwrap().len(), 1);
    let full = profile_response(UserInfo {
        subject: PrincipalId::from_u128(1).unwrap(),
        profile: Some(Names {
            given: "Ada".into(),
            family: "Lovelace".into(),
        }),
        email: Some("ada@example.com".into()),
    });
    assert_eq!(full["name"], "Ada Lovelace");
    assert_eq!(full["email_verified"], false);
    assert!(full.get("active").is_none());
    assert!(full.get("roles").is_none());
}
#[test]
fn inactive_introspection_discloses_nothing_and_active_metadata_has_no_profile() {
    assert_eq!(
        introspection_response(None, "https://issuer.example"),
        serde_json::json!({"active":false})
    );
    let value = introspection_response(
        Some(ActiveToken {
            subject: PrincipalId::from_u128(1).unwrap(),
            client: ClientId::from_u128(2).unwrap(),
            scope: "openid email".into(),
            issued: 1000,
            expires: 1300,
        }),
        "https://issuer.example",
    );
    assert_eq!(value["aud"], "https://issuer.example/userinfo");
    assert_eq!(value["active"], true);
    assert!(value.get("email").is_none());
    assert!(value.get("name").is_none());
    assert!(value.get("capabilities").is_none());
}
