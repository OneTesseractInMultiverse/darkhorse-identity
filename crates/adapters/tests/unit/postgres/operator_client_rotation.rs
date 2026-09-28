use super::*;
use darkhorse_application::registration::ClientRecord;
use darkhorse_domain::{
    identity::{ApplicationId, ClientId, ClientSecretId},
    registration::{ClientSpec, Label, Redirects},
};

fn record(application: u128, client: u128) -> Record {
    Record::Client(ClientRecord {
        id: ClientId::from_u128(client).unwrap(),
        application: ApplicationId::from_u128(application).unwrap(),
        revision: 8,
        spec: ClientSpec::new(
            Label::new("Test client").unwrap(),
            true,
            Redirects::from_validated_urls(vec!["https://example.test/cb".into()]).unwrap(),
            vec![],
            vec![],
            "client_secret_basic",
        )
        .unwrap(),
        secrets: vec![],
    })
}

#[test]
fn rotation_result_requires_the_exact_client_and_application_scope() {
    let application = ApplicationId::from_u128(1).unwrap();
    let client = ClientId::from_u128(2).unwrap();
    let secret_id = ClientSecretId::from_u128(3).unwrap();
    let secret = Zeroizing::new("private-secret".to_owned());
    let result = rotated(record(1, 2), application, client, secret_id, secret).unwrap();
    assert_eq!(result.revision, 8);
    assert_eq!(result.secret_id, secret_id);
    assert_eq!(&*result.secret, "private-secret");

    assert!(matches!(
        rotated(
            record(1, 4),
            application,
            client,
            secret_id,
            Zeroizing::new("unused".to_owned()),
        ),
        Err(Error::Unavailable)
    ));
}

#[test]
fn registration_failures_map_to_operator_outcomes_without_secret_delivery() {
    for (source, expected) in [
        (RegistrationError::Invalid, Error::Invalid),
        (RegistrationError::NotFound, Error::NotFound),
        (RegistrationError::Conflict, Error::Conflict),
        (RegistrationError::Unauthorized, Error::Denied),
        (RegistrationError::Forbidden, Error::Denied),
        (
            RegistrationError::RecentAuthenticationRequired,
            Error::Denied,
        ),
        (RegistrationError::Unavailable, Error::Unavailable),
    ] {
        assert_eq!(registration_error(source), expected);
    }
}
