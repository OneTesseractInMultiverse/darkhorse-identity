use super::*;

fn fixture() -> Projection {
    Projection {
        resource_id: uuid(4),
        application_id: uuid(3),
        active: true,
        client_active: true,
        principal_active: true,
        credential_epoch: 2,
        capabilities: vec![uuid(10), uuid(11)],
        historical: vec![uuid(12)],
        roles: vec![(uuid(20), vec![uuid(10), uuid(11)])],
        scopes: vec![(uuid(30), vec![uuid(10)])],
    }
}
fn policy(projection: &Projection) -> Result<Policy, Error> {
    assemble(
        projection,
        PrincipalId::from_u128(1).unwrap(),
        ClientId::from_u128(2).unwrap(),
        &["openid".into(), "operate".into()],
    )
}
fn grant() -> StoredGrant {
    StoredGrant {
        credential: CredentialId::from_u128(40).unwrap(),
        resource: ResourceId::from_u128(4).unwrap(),
        epoch: 2,
        ceiling: capabilities(&[uuid(10), uuid(11), uuid(12)]).unwrap(),
        created: 1000,
        expires: 10000,
    }
}
#[test]
fn projected_authority_intersects_roles_scopes_and_the_credential_ceiling() {
    let p = policy(&fixture()).unwrap();
    assert_eq!(
        p.evaluate(&grant(), 5000).unwrap(),
        capabilities(&[uuid(10)]).unwrap()
    );
    let mut reduced = grant();
    reduced
        .ceiling
        .remove(&CapabilityId::from_u128(10).unwrap());
    assert!(p.evaluate(&reduced, 5000).unwrap().is_empty());
}
#[test]
fn historical_definitions_do_not_resurrect_removed_authority() {
    let mut projection = fixture();
    projection.capabilities = vec![uuid(11)];
    projection.historical.push(uuid(10));
    projection.roles[0].1 = vec![uuid(11)];
    projection.scopes[0].1.clear();
    assert!(
        policy(&projection)
            .unwrap()
            .evaluate(&grant(), 5000)
            .unwrap()
            .is_empty()
    );
    projection.historical.clear();
    assert_eq!(
        policy(&projection).unwrap().evaluate(&grant(), 5000),
        Err(Error::InvalidToken)
    );
}
#[test]
fn inactive_and_stale_facts_cannot_authorize() {
    for mutate in [
        |p: &mut Projection| p.active = false,
        |p: &mut Projection| p.client_active = false,
        |p: &mut Projection| p.principal_active = false,
        |p: &mut Projection| p.credential_epoch += 1,
    ] {
        let mut projection = fixture();
        mutate(&mut projection);
        assert_eq!(
            policy(&projection).unwrap().evaluate(&grant(), 5000),
            Err(Error::InvalidToken)
        );
    }
    let p = policy(&fixture()).unwrap();
    for now in [999, 10000] {
        assert_eq!(p.evaluate(&grant(), now), Err(Error::InvalidToken));
    }
    let mut foreign = grant();
    foreign.resource = ResourceId::from_u128(5).unwrap();
    assert_eq!(p.evaluate(&foreign, 5000), Err(Error::InvalidToken));
}
#[test]
fn omitted_scope_or_assignment_cannot_broaden_access() {
    let mut projection = fixture();
    projection.scopes.clear();
    assert!(matches!(policy(&projection), Err(Error::InvalidGrant)));
    let mut projection = fixture();
    projection.roles.clear();
    assert!(
        policy(&projection)
            .unwrap()
            .evaluate(&grant(), 5000)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn overflow_sentinels_fail_instead_of_becoming_partial_authority() {
    for mutate in [
        |p: &mut Projection| p.capabilities = vec![uuid(10); 257],
        |p: &mut Projection| p.roles = vec![(uuid(20), vec![]); 65],
        |p: &mut Projection| p.historical = vec![uuid(12); 257],
    ] {
        let mut projection = fixture();
        mutate(&mut projection);
        assert!(matches!(policy(&projection), Err(Error::Unavailable)));
    }
}
#[test]
fn invalid_projection_identifiers_epochs_and_bindings_fail_closed() {
    for mutate in [
        |p: &mut Projection| p.resource_id = Uuid::nil(),
        |p: &mut Projection| p.application_id = Uuid::nil(),
        |p: &mut Projection| p.credential_epoch = -1,
        |p: &mut Projection| p.roles[0].0 = Uuid::nil(),
        |p: &mut Projection| p.scopes[0].0 = Uuid::nil(),
        |p: &mut Projection| p.capabilities[0] = Uuid::nil(),
        |p: &mut Projection| p.roles[0].1.push(uuid(99)),
        |p: &mut Projection| p.scopes[0].1.push(uuid(99)),
        |p: &mut Projection| p.roles.push(p.roles[0].clone()),
    ] {
        let mut projection = fixture();
        mutate(&mut projection);
        assert!(matches!(policy(&projection), Err(Error::Unavailable)));
    }
}
