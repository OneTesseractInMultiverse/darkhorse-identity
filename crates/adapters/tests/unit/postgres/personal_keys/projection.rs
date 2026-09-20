use super::*;
fn row() -> Projection {
    Projection {
        resource_id: uuid(2),
        application_id: uuid(1),
        resource_name: "API".into(),
        application_name: "Application".into(),
        active: true,
        principal_active: true,
        credential_epoch: 3,
        capabilities: vec![uuid(4)],
        roles: vec![(uuid(5), vec![uuid(4)])],
        historical: vec![uuid(6)],
    }
}
#[test]
fn bounded_projection_preserves_history_without_granting_it() {
    let policy = assemble(row(), PrincipalId::from_u128(7).unwrap()).unwrap();
    assert_eq!(policy.principal.credential_epoch, 3);
    let plan = policy
        .plan(policy.target.application, CapabilitySelection::All)
        .unwrap();
    assert_eq!(plan.ceiling, [CapabilityId::from_u128(4).unwrap()].into());
    assert_eq!(
        policy
            .plan(
                ApplicationId::from_u128(9).unwrap(),
                CapabilitySelection::All
            )
            .err(),
        Some(Error::Forbidden)
    );
    assert_eq!(
        policy
            .plan(
                policy.target.application,
                CapabilitySelection::Subset([CapabilityId::from_u128(6).unwrap()].into())
            )
            .err(),
        Some(Error::Forbidden)
    );
    for field in 0..10 {
        let mut input = row();
        match field {
            0 => input.capabilities = vec![uuid(4); 257],
            1 => input.roles = vec![(uuid(5), vec![]); 65],
            2 => input.historical = vec![uuid(6); 257],
            3 => input.application_id = uuid(0),
            4 => input.resource_id = uuid(0),
            5 => input.credential_epoch = -1,
            6 => input.roles[0].0 = uuid(0),
            7 => input.capabilities[0] = uuid(0),
            8 => input.roles[0].1 = vec![uuid(0)],
            _ => input.roles[0].1 = vec![uuid(99)],
        }
        assert_eq!(
            assemble(input, PrincipalId::from_u128(7).unwrap())
                .err()
                .unwrap(),
            Error::Unavailable
        );
    }
    let mut inactive = row();
    inactive.active = false;
    inactive.principal_active = false;
    let policy = assemble(inactive, PrincipalId::from_u128(7).unwrap()).unwrap();
    assert_eq!(
        policy
            .plan(policy.target.application, CapabilitySelection::All)
            .err(),
        Some(Error::Forbidden)
    );
}
