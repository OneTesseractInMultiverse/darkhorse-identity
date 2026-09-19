use super::*;
fn id(n: u128) -> SessionId {
    SessionId::from_u128(n).unwrap()
}
fn owner(n: u128) -> PrincipalId {
    PrincipalId::from_u128(n).unwrap()
}
fn facts() -> SessionFacts {
    SessionFacts {
        active: true,
        credential_live: true,
        revoked: false,
        issued_epoch: 0,
        current_epoch: 0,
        created_ms: 1000,
        seen_ms: 1000,
        expires_ms: 1000 + crate::authentication::ABSOLUTE_MS,
    }
}
#[test]
fn only_a_live_owner_can_manage_sessions() {
    assert_eq!(authenticate(facts(), 2000), Ok(()));
    for f in [
        SessionFacts {
            revoked: true,
            ..facts()
        },
        SessionFacts {
            active: false,
            ..facts()
        },
        SessionFacts {
            credential_live: false,
            ..facts()
        },
        SessionFacts {
            current_epoch: 1,
            ..facts()
        },
    ] {
        assert_eq!(authenticate(f, 2000), Err(Error::Unauthorized));
        assert_eq!(status(f, 2000), Status::Inactive);
    }
    assert_eq!(authenticate(facts(), 901000), Err(Error::Unauthorized));
    assert_eq!(authenticate(facts(), 999), Err(Error::Unauthorized));
    assert_eq!(status(facts(), 2000), Status::Active);
    assert_eq!(owns(owner(1), owner(1)), Ok(()));
    assert_eq!(owns(owner(1), owner(2)), Err(Error::NotFound));
}
#[test]
fn pagination_is_bounded_and_cursors_follow_the_last_returned_record() {
    let record = |n| Record {
        id: id(n),
        created_ms: 2000,
        seen_ms: 2000,
        expires_ms: 3000,
        status: Status::Active,
    };
    let small = page(id(1), vec![record(1)]).unwrap();
    assert!(small.next.is_none());
    assert_eq!(small.current, id(1));
    assert!(page(id(1), vec![]).unwrap().items.is_empty());
    let full = page(id(1), (1..=26).map(record).collect()).unwrap();
    assert_eq!(full.items.len(), 25);
    assert_eq!(
        full.next,
        Some(Cursor {
            created_ms: 2000,
            id: id(25)
        })
    );
    assert_eq!(
        page(id(1), (1..=27).map(record).collect()),
        Err(Error::Unavailable)
    );
    assert_eq!(
        Cursor::new(i64::MAX as u64, id(1)),
        Ok(Cursor {
            created_ms: i64::MAX as u64,
            id: id(1)
        })
    );
    assert_eq!(Cursor::new(i64::MAX as u64 + 1, id(1)), Err(Error::Invalid));
}
