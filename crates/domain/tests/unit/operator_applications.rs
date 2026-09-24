use super::*;
use crate::{identity::PrincipalId, registration::Label};
fn spec() -> ApplicationSpec {
    ApplicationSpec {
        name: Label::new("Portal").unwrap(),
        owner: PrincipalId::from_u128(1).unwrap(),
        active: true,
    }
}
#[test]
fn mutation_reasons_and_revisions_are_bounded_without_changing_registration_policy() {
    let create = Operation::Create(spec());
    let request = Request::new(create.clone(), "  Approved onboarding  ").unwrap();
    assert_eq!(request.operation(), &create);
    assert_eq!(request.reason(), "Approved onboarding");
    for reason in [
        "".into(),
        "\n".into(),
        "a\nb".into(),
        "a\u{202e}b".into(),
        "x".repeat(201),
        "😀".repeat(129),
    ] {
        assert!(matches!(
            Request::new(create.clone(), &reason),
            Err(Error::Invalid)
        ));
    }
    for revision in [0, i64::MAX as u64] {
        let update = Operation::Update {
            application: ApplicationId::from_u128(2).unwrap(),
            revision,
            spec: spec(),
        };
        assert!(Request::new(update, &"x".repeat(200)).is_ok());
    }
    assert!(matches!(
        Request::new(
            Operation::Update {
                application: ApplicationId::from_u128(2).unwrap(),
                revision: i64::MAX as u64 + 1,
                spec: spec()
            },
            "Approved"
        ),
        Err(Error::Invalid)
    ));
}
