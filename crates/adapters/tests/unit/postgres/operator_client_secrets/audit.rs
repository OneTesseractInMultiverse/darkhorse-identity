use super::*;
#[test]
fn audit_accepts_only_operation_matched_definite_results() {
    let list = Operation::List {
        after: None,
        limit: 1,
    };
    let retire = Operation::Retire {
        secret: ClientSecretId::from_u128(1).unwrap(),
        revision: 0,
    };
    let listed = Outcome::Listed(Inventory {
        revision: 0,
        observed_ms: 10,
        items: vec![],
        next: None,
    });
    let retired = Outcome::Retired { revision: 1 };
    assert_eq!(result(list, &Ok(listed.clone())), Ok(("read", Some(0))));
    assert_eq!(result(retire, &Ok(retired.clone())), Ok(("written", None)));
    assert_eq!(result(list, &Ok(retired)), Err(Error::Unavailable));
    assert_eq!(result(retire, &Ok(listed)), Err(Error::Unavailable));
    let over = Outcome::Listed(Inventory {
        revision: 0,
        observed_ms: 0,
        items: (1..=2)
            .map(|n| Metadata {
                id: ClientSecretId::from_u128(n).unwrap(),
                created_ms: 0,
                expires_ms: None,
                retired: false,
            })
            .collect(),
        next: None,
    });
    assert_eq!(result(list, &Ok(over)), Err(Error::Unavailable));
    for op in [list, retire] {
        for error in [
            Error::Invalid,
            Error::Unavailable,
            Error::Uncertain,
            Error::PolicyRejected,
            Error::Limited { retry_after_ms: 1 },
        ] {
            assert_eq!(result(op, &Err(error)), Err(Error::Unavailable));
        }
        assert_eq!(result(op, &Err(Error::Denied)), Ok(("denied", None)));
        assert_eq!(result(op, &Err(Error::NotFound)), Ok(("not_found", None)));
    }
    assert_eq!(result(list, &Err(Error::Conflict)), Err(Error::Unavailable));
    assert_eq!(
        result(retire, &Err(Error::Conflict)),
        Ok(("conflict", None))
    );
}
