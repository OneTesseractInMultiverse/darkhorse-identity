use super::*;

macro_rules! identifier_contract {
    ($test:ident, $name:ident) => {
        #[test]
        fn $test() {
            assert_eq!($name::from_u128(0), Err(InvalidIdentifier));
            for value in [1, 42, u128::MAX] {
                assert_eq!($name::from_u128(value).unwrap().as_u128(), value);
            }
        }
    };
}

identifier_contract!(principal_identifier_contract, PrincipalId);
identifier_contract!(application_identifier_contract, ApplicationId);
identifier_contract!(resource_identifier_contract, ResourceId);
identifier_contract!(role_identifier_contract, RoleId);
identifier_contract!(capability_identifier_contract, CapabilityId);
identifier_contract!(scope_identifier_contract, ScopeId);
identifier_contract!(client_identifier_contract, ClientId);
identifier_contract!(credential_identifier_contract, CredentialId);

identifier_contract!(session_identifier_contract, SessionId);
