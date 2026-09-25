use super::*;
fn target() -> Target {
    Target {
        application: ApplicationId::from_u128(1).unwrap(),
        client: ClientId::from_u128(2).unwrap(),
    }
}
#[test]
fn metadata_pages_are_bounded_and_retirement_requires_a_valid_reason_and_revision() {
    let secret = ClientSecretId::from_u128(3).unwrap();
    for limit in [1, 25] {
        let r = Request::new(
            target(),
            Operation::List {
                after: Some(secret),
                limit,
            },
            None,
        )
        .unwrap();
        assert_eq!(r.target(), target());
        assert_eq!(r.reason(), None);
    }
    for limit in [0, 26, u16::MAX] {
        assert_eq!(
            Request::new(target(), Operation::List { after: None, limit }, None),
            Err(Error::Invalid)
        );
    }
    assert!(
        Request::new(
            target(),
            Operation::List {
                after: None,
                limit: 1
            },
            Some("Reason")
        )
        .is_err()
    );
    let op = Operation::Retire {
        secret,
        revision: i64::MAX as u64,
    };
    assert_eq!(
        Request::new(target(), op, Some(" Approved "))
            .unwrap()
            .reason(),
        Some("Approved")
    );
    for reason in [None, Some(""), Some("a\nb"), Some("a\u{202e}b")] {
        assert!(Request::new(target(), op, reason).is_err());
    }
    assert!(
        Request::new(
            target(),
            Operation::Retire {
                secret,
                revision: i64::MAX as u64 + 1
            },
            Some("Reason")
        )
        .is_err()
    );
}
