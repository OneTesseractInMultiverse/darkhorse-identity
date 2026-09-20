use super::*;

fn resource(n: u128) -> ResourceId {
    ResourceId::from_u128(n).unwrap()
}
fn request(grants: Vec<Selection>) -> Result<Request, Error> {
    Request::new(
        " build worker ",
        ApplicationId::from_u128(1).unwrap(),
        7,
        Expiration::Default,
        grants,
    )
}
fn all(n: u128) -> Selection {
    Selection {
        resource: resource(n),
        capabilities: CapabilitySelection::All,
    }
}

#[test]
fn expiration_policy_preserves_choice_with_bounded_expiring_default() {
    let policy = ExpiryPolicy::new(30, 365, true).unwrap();
    assert_eq!(
        (
            policy.default_days(),
            policy.maximum_days(),
            policy.allow_never()
        ),
        (30, 365, true)
    );
    assert_eq!(
        policy.deadline(Expiration::Default, 1000),
        Ok(Some(1000 + 30 * DAY_MS))
    );
    assert_eq!(
        policy.deadline(Expiration::Days(1), 1000),
        Ok(Some(1000 + DAY_MS))
    );
    assert_eq!(policy.deadline(Expiration::Never, 1000), Ok(None));
    for days in [0, 366] {
        assert_eq!(
            policy.deadline(Expiration::Days(days), 0),
            Err(Error::Invalid)
        );
    }
    assert_eq!(ExpiryPolicy::new(0, 365, true), Err(Error::Invalid));
    assert_eq!(ExpiryPolicy::new(366, 365, true), Err(Error::Invalid));
    assert_eq!(ExpiryPolicy::new(1, 3651, true), Err(Error::Invalid));
    assert_eq!(
        ExpiryPolicy::new(30, 365, false)
            .unwrap()
            .deadline(Expiration::Never, 0),
        Err(Error::Forbidden)
    );
    assert_eq!(
        policy.deadline(Expiration::Default, MAX_TIME),
        Err(Error::Invalid)
    );
    assert_eq!(
        policy.deadline(Expiration::Never, MAX_TIME + 1),
        Err(Error::Invalid)
    );
}

#[test]
fn issuance_requires_bounded_explicit_distinct_resource_grants() {
    assert_eq!(request(vec![]).err(), Some(Error::Invalid));
    assert_eq!(request(vec![all(1), all(1)]).err(), Some(Error::Invalid));
    assert_eq!(
        request((1..=17).map(all).collect()).err(),
        Some(Error::Invalid)
    );
    assert_eq!(
        request(vec![Selection {
            resource: resource(1),
            capabilities: CapabilitySelection::Subset(Default::default())
        }])
        .err(),
        Some(Error::Invalid)
    );
    let too_many = (1..=257)
        .map(|n| CapabilityId::from_u128(n).unwrap())
        .collect();
    assert_eq!(
        request(vec![Selection {
            resource: resource(1),
            capabilities: CapabilitySelection::Subset(too_many)
        }])
        .err(),
        Some(Error::Invalid)
    );
    let valid = request(vec![all(1), all(2)]).unwrap();
    assert_eq!(valid.name().as_str(), "build worker");
    assert_eq!(valid.application().as_u128(), 1);
    assert_eq!(valid.revision(), 7);
    assert_eq!(valid.expiration(), Expiration::Default);
    assert_eq!(valid.grants().len(), 2);
    let subset = request(vec![Selection {
        resource: resource(1),
        capabilities: CapabilitySelection::Subset([CapabilityId::from_u128(1).unwrap()].into()),
    }]);
    assert!(subset.is_ok());
    assert_eq!(
        Request::new(
            "\n",
            valid.application(),
            7,
            Expiration::Never,
            vec![all(1)]
        )
        .err(),
        Some(Error::Invalid)
    );
    assert_eq!(
        Request::new(
            "key",
            valid.application(),
            u64::MAX,
            Expiration::Never,
            vec![all(1)]
        )
        .err(),
        Some(Error::Invalid)
    );
}

#[test]
fn recent_auth_revision_and_issuance_budgets_fail_closed() {
    assert_eq!(recent(1000, 1000), Ok(()));
    assert_eq!(recent(1000, 300999), Ok(()));
    assert_eq!(
        recent(1000, 301000),
        Err(Error::RecentAuthenticationRequired)
    );
    assert_eq!(recent(1000, 999), Err(Error::RecentAuthenticationRequired));
    assert_eq!(revision(7, 7), Ok(()));
    assert_eq!(revision(7, 8), Err(Error::Conflict));
    assert_eq!(capacity(99, 9), Ok(()));
    assert_eq!(capacity(100, 0), Err(Error::Limit));
    assert_eq!(capacity(0, 10), Err(Error::Limit));
    assert!(live(1000, Some(2000), 1999));
    assert!(!live(1000, Some(2000), 2000));
    assert!(!live(1000, None, 999));
    assert!(!live(1000, Some(1000), 1000));
    assert!(live(1000, None, MAX_TIME));
}
