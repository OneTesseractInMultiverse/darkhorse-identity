use super::*;
use crate::{
    identity::ResourceId,
    registration::{Label, Redirects},
};
fn update() -> Update {
    Update {
        application: ApplicationId::from_u128(1).unwrap(),
        client: ClientId::from_u128(2).unwrap(),
        revision: 0,
        spec: ClientSpec::new(
            Label::new("Portal").unwrap(),
            false,
            Redirects::from_validated_urls(vec!["https://portal.example/cb".into()]).unwrap(),
            vec![],
            vec![],
            "client_secret_basic",
        )
        .unwrap(),
    }
}
#[test]
fn complete_requests_preserve_policy_and_reject_invalid_revisions_reasons_and_allowances() {
    let mut value = update();
    value.spec.refresh_tokens = true;
    let request = Request::new(value, "  Approved change  ").unwrap();
    assert!(request.update().spec.refresh_tokens);
    assert!(!request.update().spec.active);
    assert_eq!(request.reason(), "Approved change");
    for reason in [
        "".into(),
        "a\nb".into(),
        "a\u{202e}b".into(),
        "x".repeat(201),
        "😀".repeat(129),
    ] {
        assert!(matches!(
            Request::new(update(), &reason),
            Err(Error::Invalid)
        ));
    }
    let mut value = update();
    value.revision = i64::MAX as u64;
    assert!(Request::new(value.clone(), "Approved").is_ok());
    value.revision += 1;
    assert!(matches!(
        Request::new(value, "Approved"),
        Err(Error::Invalid)
    ));
    let mut value = update();
    value.spec.resources = vec![ResourceId::from_u128(3).unwrap(); 2];
    assert!(matches!(
        Request::new(value, "Approved"),
        Err(Error::Invalid)
    ));
}
