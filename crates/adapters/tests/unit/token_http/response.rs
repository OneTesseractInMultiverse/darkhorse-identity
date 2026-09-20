use super::*;
use darkhorse_domain::identity::{ClientId, PrincipalId};
#[test]
fn personal_keys_disclose_resource_authority_without_oauth_identity_or_fake_expiration() {
    use darkhorse_domain::identity::{CapabilityId, CredentialId, ResourceId};
    assert_eq!(
        key_response(None, "https://issuer.example"),
        serde_json::json!({"active":false})
    );
    for expires in [None, Some(1300)] {
        let response = key_response(
            Some(darkhorse_application::personal_keys::Active {
                credential: CredentialId::from_u128(4).unwrap(),
                subject: PrincipalId::from_u128(1).unwrap(),
                resource: ResourceId::from_u128(2).unwrap(),
                capabilities: [CapabilityId::from_u128(3).unwrap()].into(),
                issued: 1000,
                expires,
            }),
            "https://issuer.example",
        );
        assert_eq!(
            response.get("exp").and_then(serde_json::Value::as_u64),
            expires
        );
        for omitted in [
            "client_id",
            "scope",
            "email",
            "name",
            "secret",
            "credential_id",
        ] {
            assert!(response.get(omitted).is_none());
        }
        assert_eq!(response["credential_type"], "personal_key");
        assert_eq!(response["capabilities"].as_array().unwrap().len(), 1);
    }
}
#[test]
fn refresh_responses_omit_id_tokens_and_disabled_clients_receive_no_refresh() {
    let fields = token_fields(Tokens {
        access: "access".into(),
        refresh: Some("refresh".into()),
        id_token: None,
        expires_in: 300,
        scope: "openid".into(),
    });
    assert!(fields.get("id_token").is_none());
    assert_eq!(fields["refresh_token"], "refresh");
    let fields = token_fields(Tokens {
        access: "access".into(),
        refresh: None,
        id_token: Some("signed".into()),
        expires_in: 300,
        scope: "openid".into(),
    });
    assert!(fields.get("refresh_token").is_none());
    assert_eq!(fields["id_token"], "signed");
    assert_eq!(
        metadata("https://issuer.example")["grant_types_supported"],
        serde_json::json!(["authorization_code", "refresh_token"])
    );
}
#[test]
fn profile_projection_omits_unapproved_fields_and_uses_authoritative_email_verification() {
    let profile = UserInfo {
        email_verified: false,
        subject: PrincipalId::from_u128(1).unwrap(),
        profile: None,
        email: None,
    };
    let result = profile_response(profile);
    assert_eq!(result.as_object().unwrap().len(), 1);
    let full = profile_response(UserInfo {
        email_verified: true,
        subject: PrincipalId::from_u128(1).unwrap(),
        profile: Some(Names {
            given: "Ada".into(),
            family: "Lovelace".into(),
        }),
        email: Some("ada@example.com".into()),
    });
    assert_eq!(full["name"], "Ada Lovelace");
    assert_eq!(full["email_verified"], true);
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

#[test]
fn resource_projection_discloses_only_the_exact_audience_and_effective_capabilities() {
    use darkhorse_domain::identity::{CapabilityId, ResourceId};
    assert_eq!(
        resource_response(None, "https://issuer.example"),
        serde_json::json!({"active":false})
    );
    let response = resource_response(
        Some(ActiveResourceToken {
            token: ActiveToken {
                subject: PrincipalId::from_u128(1).unwrap(),
                client: ClientId::from_u128(2).unwrap(),
                scope: "openid operate".into(),
                issued: 1000,
                expires: 1300,
            },
            resource: ResourceId::from_u128(3).unwrap(),
            capabilities: std::collections::BTreeSet::from([
                CapabilityId::from_u128(5).unwrap(),
                CapabilityId::from_u128(4).unwrap(),
            ]),
        }),
        "https://issuer.example",
    );
    assert_eq!(
        response,
        serde_json::json!({"active":true,"token_type":"Bearer","iss":"https://issuer.example","aud":"urn:darkhorse:resource:00000000-0000-0000-0000-000000000003","client_id":"00000000-0000-0000-0000-000000000002","sub":"00000000-0000-0000-0000-000000000001","scope":"openid operate","iat":1000,"exp":1300,"capabilities":["00000000-0000-0000-0000-000000000004","00000000-0000-0000-0000-000000000005"]})
    );
}
