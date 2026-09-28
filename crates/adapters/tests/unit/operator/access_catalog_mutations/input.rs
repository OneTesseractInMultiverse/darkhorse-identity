use super::*;
use darkhorse_domain::{admin_catalog::Change, identity::ApplicationId};
use std::io::Cursor;

const AUTH: &str = r#""authentication":{"email":"admin@example.com","password":"never-echo-this","reason":"Create a reader role"}"#;

#[test]
fn protected_input_builds_a_typed_revision_fenced_change() {
    let body = format!(
        "{{{AUTH},\"policy_revision\":\"7\",\"change\":{{\"operation\":\"create_role\",\"name\":\"Reader\",\"application_id\":\"00000000-0000-0000-0000-000000000007\"}}}}"
    );
    let (request, authentication) = read(Cursor::new(body)).unwrap();
    assert_eq!(request.policy_revision(), 7);
    assert_eq!(request.reason(), "Create a reader role");
    assert!(matches!(
        request.change(),
        Change::CreateRole { name, application: Some(application) }
            if name.as_str() == "Reader" && *application == ApplicationId::from_u128(7).unwrap()
    ));
    assert_eq!(authentication.email, "admin@example.com");
    assert_eq!(authentication.password, "never-echo-this");
}

#[test]
fn malformed_or_unbounded_input_fails_without_reflecting_credentials() {
    let valid = format!(
        "{{{AUTH},\"policy_revision\":\"7\",\"change\":{{\"operation\":\"create_role\",\"name\":\"Reader\"}}}}"
    );
    for body in [
        valid.replace("\"name\":\"Reader\"", "\"name\":\"Reader\",\"extra\":true"),
        valid.replace(
            "\"policy_revision\":\"7\"",
            "\"policy_revision\":9223372036854775808",
        ),
        valid.replace("\"reason\":\"Create a reader role\"", "\"reason\":\"\""),
    ] {
        let error = match read(Cursor::new(body)) {
            Ok(_) => panic!("invalid protected input was accepted"),
            Err(error) => error,
        };
        assert!(!format!("{error:?}").contains("never-echo-this"));
    }
    assert!(read(Cursor::new(" ".repeat(32 * 1024 + 1))).is_err());
}

#[test]
fn principal_role_input_requires_typed_ids_and_both_revisions() {
    let body = format!(
        "{{{AUTH},\"policy_revision\":\"7\",\"change\":{{\"operation\":\"principal_role\",\"principal_id\":\"00000000-0000-0000-0000-000000000001\",\"application_id\":\"00000000-0000-0000-0000-000000000007\",\"role_id\":\"00000000-0000-0000-0000-000000000008\",\"assigned\":true,\"principal_revision\":\"9\"}}}}"
    );
    let (request, _) = read(Cursor::new(body.clone())).unwrap();
    assert_eq!(request.policy_revision(), 7);
    assert_eq!(request.change().principal_revision(), Some(9));
    assert!(matches!(
        request.change(),
        Change::PrincipalRole { principal, application, role, assigned: true, principal_revision: 9 }
            if *principal == darkhorse_domain::identity::PrincipalId::from_u128(1).unwrap()
                && *application == ApplicationId::from_u128(7).unwrap()
                && *role == darkhorse_domain::identity::RoleId::from_u128(8).unwrap()
    ));
    for invalid in [
        body.replace(
            "\"principal_revision\":\"9\"",
            "\"principal_revision\":9223372036854775808",
        ),
        body.replace("00000000-0000-0000-0000-000000000008", "invalid"),
        body.replace("\"assigned\":true,", ""),
    ] {
        assert!(read(Cursor::new(invalid)).is_err());
    }
}
