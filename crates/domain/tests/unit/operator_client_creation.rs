use super::*;
use crate::{
    identity::{ResourceId, ScopeId},
    registration::{ClientSpec, Label, Redirects},
};

fn spec() -> ClientSpec {
    ClientSpec::new(
        Label::new("Web portal").unwrap(),
        true,
        Redirects::from_validated_urls(vec!["https://portal.example/callback".into()]).unwrap(),
        vec![ResourceId::from_u128(3).unwrap()],
        vec![ScopeId::from_u128(4).unwrap()],
        "client_secret_basic",
    )
    .unwrap()
}

#[test]
fn request_keeps_scoped_configuration_and_validates_reason() {
    let request = Request::new(
        ApplicationId::from_u128(1).unwrap(),
        spec(),
        " Approved registration ",
    )
    .unwrap();

    assert_eq!(request.application().as_u128(), 1);
    assert_eq!(request.spec().name.as_str(), "Web portal");
    assert_eq!(request.reason(), "Approved registration");
    for reason in ["".to_owned(), "a\nb".into(), "x".repeat(201)] {
        assert!(Request::new(ApplicationId::from_u128(1).unwrap(), spec(), &reason).is_err());
    }
}
