use super::*;
use darkhorse_application::registration::ApplicationRecord;
use darkhorse_domain::identity::{ApplicationId, ClientId, PrincipalId};
#[test]
fn mismatched_read_or_record_cannot_be_treated_as_completed_retirement() {
    let request = Request::new(
        Target {
            application: ApplicationId::from_u128(1).unwrap(),
            client: ClientId::from_u128(2).unwrap(),
        },
        Operation::List {
            after: None,
            limit: 1,
        },
        None,
    )
    .unwrap();
    assert!(matches!(retirement(&request), Err(Error::Invalid)));
    let record = Record::Application(ApplicationRecord {
        id: ApplicationId::from_u128(1).unwrap(),
        revision: 0,
        name: "Application".into(),
        owner: PrincipalId::from_u128(2).unwrap(),
        owner_email: "owner@example.com".into(),
        active: true,
    });
    assert_eq!(retired(record), Err(Error::Unavailable));
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
}
