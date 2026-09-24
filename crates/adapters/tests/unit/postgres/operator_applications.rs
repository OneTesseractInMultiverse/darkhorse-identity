use super::*;
use darkhorse_application::registration::ResourceRecord;
use darkhorse_domain::identity::{ApplicationId, ResourceId};

#[test]
fn unrelated_registration_records_and_authority_errors_cannot_become_success() {
    let record = Record::Resource(ResourceRecord {
        id: ResourceId::from_u128(2).unwrap(),
        application: ApplicationId::from_u128(1).unwrap(),
        name: "Unrelated resource".into(),
        audience: "urn:unrelated".into(),
    });
    assert!(matches!(application(record), Err(Error::Unavailable)));
    for error in [
        RegistrationError::Unauthorized,
        RegistrationError::Forbidden,
        RegistrationError::RecentAuthenticationRequired,
    ] {
        assert_eq!(registration_error(error), Error::Denied);
    }
}
