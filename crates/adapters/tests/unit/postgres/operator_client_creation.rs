use super::*;

#[test]
fn registration_failures_keep_the_operator_error_contract() {
    for error in [
        RegistrationError::Unauthorized,
        RegistrationError::Forbidden,
        RegistrationError::RecentAuthenticationRequired,
    ] {
        assert_eq!(registration_error(error), Error::Denied);
    }
    assert_eq!(
        registration_error(RegistrationError::Invalid),
        Error::Invalid
    );
    assert_eq!(
        registration_error(RegistrationError::NotFound),
        Error::NotFound
    );
    assert_eq!(
        registration_error(RegistrationError::Conflict),
        Error::Conflict
    );
    assert_eq!(
        registration_error(RegistrationError::Unavailable),
        Error::Unavailable
    );
}

#[test]
fn unrelated_records_cannot_be_returned_as_created_clients() {
    let record = Record::Application(darkhorse_application::registration::ApplicationRecord {
        id: darkhorse_domain::identity::ApplicationId::from_u128(1).unwrap(),
        name: "Unrelated application".into(),
        owner: darkhorse_domain::identity::PrincipalId::from_u128(2).unwrap(),
        owner_email: "private@example.test".into(),
        active: true,
        revision: 0,
    });
    assert!(matches!(client(record), Err(Error::Unavailable)));
}
