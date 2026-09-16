use super::*;
use crate::{AccountStatus, authorization::fixtures::*};
use std::collections::BTreeSet;

fn evaluate(
    defs: Definitions,
    user: &Principal,
    grant: &CredentialGrant,
    requested: Target,
    now: u64,
) -> Result<CapabilitySet, Denial> {
    effective_capabilities(
        &Catalog::new(defs).unwrap(),
        &Evaluation {
            principal: user,
            credential: grant,
            target: requested,
            now,
        },
    )
}

#[test]
fn read_scope_restricts_writer_and_unknown_capabilities_are_denied() {
    let catalog = Catalog::new(definitions()).unwrap();
    let user = principal();
    let grant = credential();
    let input = Evaluation {
        principal: &user,
        credential: &grant,
        target: target(),
        now: 100,
    };
    assert_eq!(effective_capabilities(&catalog, &input), Ok(caps(&[1])));
    assert_eq!(authorize(&catalog, &input, cap(1)), Ok(()));
    for denied in [2, 3, 4, 99] {
        assert_eq!(
            authorize(&catalog, &input, cap(denied)),
            Err(Denial::InsufficientAccess)
        );
    }
    let input = Evaluation { now: 150, ..input };
    assert_eq!(
        authorize(&catalog, &input, cap(1)),
        Err(Denial::ExpiredCredential)
    );
}

#[test]
fn credential_binding_and_lifecycle_fail_closed() {
    type Change = fn(&mut Principal, &mut CredentialGrant);
    let cases: [(Change, Denial); 10] = [
        (
            |p, _| p.status = AccountStatus::Inactive,
            Denial::InactivePrincipal,
        ),
        (|_, g| g.revoked = true, Denial::RevokedCredential),
        (|_, g| g.subject = principal_id(2), Denial::SubjectMismatch),
        (|p, _| p.credential_epoch += 1, Denial::StaleCredential),
        (|_, g| g.principal_epoch += 1, Denial::StaleCredential),
        (
            |_, g| g.target.application = app(2),
            Denial::AudienceMismatch,
        ),
        (
            |_, g| g.target.resource = resource(2),
            Denial::AudienceMismatch,
        ),
        (|_, g| g.expires_at = Some(50), Denial::InvalidCredential),
        (|_, g| g.expires_at = Some(49), Denial::InvalidCredential),
        (|_, g| g.expires_at = None, Denial::InvalidCredential),
    ];
    for (change, expected) in cases {
        let (mut user, mut grant) = (principal(), credential());
        change(&mut user, &mut grant);
        assert_eq!(
            evaluate(definitions(), &user, &grant, target(), 100),
            Err(expected)
        );
    }
}

#[test]
fn expiry_is_exclusive_activation_is_inclusive_and_keys_can_be_nonexpiring() {
    for (now, expected) in [
        (49, Err(Denial::NotYetValid)),
        (50, Ok(caps(&[1]))),
        (149, Ok(caps(&[1]))),
        (150, Err(Denial::ExpiredCredential)),
        (u64::MAX, Err(Denial::ExpiredCredential)),
    ] {
        assert_eq!(
            evaluate(definitions(), &principal(), &credential(), target(), now),
            expected
        );
    }
    let mut grant = credential();
    grant.delegation = Delegation::PersonalKey;
    grant.expires_at = None;
    assert_eq!(
        evaluate(definitions(), &principal(), &grant, target(), u64::MAX),
        Ok(caps(&[1, 2]))
    );
    grant.valid_from = u64::MAX;
    assert_eq!(
        evaluate(definitions(), &principal(), &grant, target(), u64::MAX),
        Ok(caps(&[1, 2]))
    );
    grant.valid_from = 0;
    grant.expires_at = Some(u64::MAX);
    assert_eq!(
        evaluate(definitions(), &principal(), &grant, target(), u64::MAX - 1),
        Ok(caps(&[1, 2]))
    );
    assert_eq!(
        evaluate(definitions(), &principal(), &grant, target(), u64::MAX),
        Err(Denial::ExpiredCredential)
    );
}

#[test]
fn missing_or_inactive_targets_deny() {
    for (requested, expected) in [
        (
            Target {
                application: app(99),
                resource: resource(1),
            },
            Denial::UnknownApplication,
        ),
        (
            Target {
                application: app(1),
                resource: resource(99),
            },
            Denial::UnknownResource,
        ),
        (
            Target {
                application: app(2),
                resource: resource(1),
            },
            Denial::AudienceMismatch,
        ),
    ] {
        let grant = CredentialGrant {
            target: requested,
            ..credential()
        };
        assert_eq!(
            evaluate(definitions(), &principal(), &grant, requested, 100),
            Err(expected)
        );
    }
    let mut defs = definitions();
    defs.applications[0].active = false;
    assert_eq!(
        evaluate(defs, &principal(), &credential(), target(), 100),
        Err(Denial::InactiveApplication)
    );
    let mut defs = definitions();
    defs.resources[0].active = false;
    assert_eq!(
        evaluate(defs, &principal(), &credential(), target(), 100),
        Err(Denial::InactiveResource)
    );
}

#[test]
fn invalid_assignments_cannot_be_hidden_among_valid_assignments() {
    for assignment in [
        Assignment {
            application: app(99),
            role: role(1),
        },
        Assignment {
            application: app(1),
            role: role(99),
        },
        Assignment {
            application: app(1),
            role: role(2),
        },
    ] {
        let mut user = principal();
        user.assignments.insert(assignment);
        assert_eq!(
            evaluate(definitions(), &user, &credential(), target(), 100),
            Err(Denial::InvalidAssignment)
        );
    }
}

#[test]
fn scope_and_client_changes_restrict_existing_credentials() {
    type Change = fn(&mut Definitions, &mut CredentialGrant);
    let cases: [(Change, Denial); 8] = [
        (
            |_, g| {
                g.delegation = Delegation::OAuth {
                    client: client(99),
                    scopes: BTreeSet::from([scope(1)]),
                }
            },
            Denial::UnknownClient,
        ),
        (|d, _| d.clients[0].active = false, Denial::InactiveClient),
        (
            |d, _| {
                d.clients[0].resources.clear();
                d.clients[0].scopes.clear();
            },
            Denial::ClientRestriction,
        ),
        (|d, _| d.clients[0].scopes.clear(), Denial::InvalidScope),
        (
            |_, g| {
                g.delegation = Delegation::OAuth {
                    client: client(1),
                    scopes: BTreeSet::new(),
                }
            },
            Denial::InvalidScope,
        ),
        (
            |_, g| {
                g.delegation = Delegation::OAuth {
                    client: client(1),
                    scopes: BTreeSet::from([scope(99)]),
                }
            },
            Denial::InvalidScope,
        ),
        (
            |d, _| {
                d.clients[0].application = app(2);
                d.applications[1].active = false;
            },
            Denial::InactiveApplication,
        ),
        (|_, g| g.ceiling = caps(&[99]), Denial::UnknownCapability),
    ];
    for (change, expected) in cases {
        let (mut defs, mut grant) = (definitions(), credential());
        change(&mut defs, &mut grant);
        assert_eq!(
            evaluate(defs, &principal(), &grant, target(), 100),
            Err(expected)
        );
    }
    let mut defs = definitions();
    defs.clients[0].resources.insert(resource(2));
    defs.clients[0].scopes.insert(scope(3));
    let grant = CredentialGrant {
        delegation: Delegation::OAuth {
            client: client(1),
            scopes: BTreeSet::from([scope(3)]),
        },
        ..credential()
    };
    assert_eq!(
        evaluate(defs, &principal(), &grant, target(), 100),
        Err(Denial::InvalidScope)
    );
}

#[test]
fn scopes_only_limit_and_multiple_scopes_and_roles_union() {
    let mut grant = credential();
    grant.delegation = Delegation::OAuth {
        client: client(1),
        scopes: BTreeSet::from([scope(1), scope(2)]),
    };
    assert_eq!(
        evaluate(definitions(), &principal(), &grant, target(), 100),
        Ok(caps(&[1, 2]))
    );
    let mut user = principal();
    user.assignments.clear();
    assert_eq!(
        evaluate(definitions(), &user, &grant, target(), 100),
        Ok(caps(&[]))
    );
    let mut defs = definitions();
    defs.roles[0].capabilities = caps(&[1]);
    defs.roles.push(Role {
        id: role(3),
        applications: BTreeSet::from([app(1)]),
        capabilities: caps(&[2]),
    });
    user.assignments = BTreeSet::from([
        Assignment {
            application: app(1),
            role: role(1),
        },
        Assignment {
            application: app(1),
            role: role(3),
        },
    ]);
    assert_eq!(
        evaluate(defs, &user, &grant, target(), 100),
        Ok(caps(&[1, 2]))
    );
}

#[test]
fn explicit_cross_application_client_registration_is_allowed() {
    let mut defs = definitions();
    defs.clients[0].application = app(2);
    assert_eq!(
        evaluate(defs, &principal(), &credential(), target(), 100),
        Ok(caps(&[1]))
    );
}

#[test]
fn shared_definitions_require_assignment_in_each_bound_application() {
    let mut defs = definitions();
    for capability in &mut defs.capabilities[..2] {
        capability.applications.insert(app(2));
    }
    defs.roles[0].applications.insert(app(2));
    defs.resources[1].capabilities.extend(caps(&[1, 2]));
    let second = Target {
        application: app(2),
        resource: resource(2),
    };
    let grant = CredentialGrant {
        target: second,
        delegation: Delegation::PersonalKey,
        ..credential()
    };
    assert_eq!(
        evaluate(defs.clone(), &principal(), &grant, second, 100),
        Ok(caps(&[]))
    );
    let mut user = principal();
    user.assignments.insert(Assignment {
        application: app(2),
        role: role(1),
    });
    assert_eq!(
        evaluate(defs.clone(), &user, &grant, second, 100),
        Ok(caps(&[1, 2]))
    );
    defs.resources.push(Resource {
        id: resource(3),
        application: app(3),
        active: true,
        capabilities: caps(&[]),
    });
    let third = Target {
        application: app(3),
        resource: resource(3),
    };
    let grant = CredentialGrant {
        target: third,
        ..grant
    };
    assert_eq!(evaluate(defs, &user, &grant, third, 100), Ok(caps(&[])));
}

#[test]
fn live_reductions_apply_and_later_expansion_stays_inside_ceiling() {
    let mut grant = credential();
    grant.delegation = Delegation::PersonalKey;
    let mut defs = definitions();
    defs.roles[0].capabilities = caps(&[1, 2, 3]);
    assert_eq!(
        evaluate(defs.clone(), &principal(), &grant, target(), 100),
        Ok(caps(&[1, 2]))
    );
    defs.roles[0].capabilities.remove(&cap(1));
    assert_eq!(
        evaluate(defs.clone(), &principal(), &grant, target(), 100),
        Ok(caps(&[2]))
    );
    defs.resources[0].capabilities.remove(&cap(2));
    defs.scopes[1].capabilities.clear();
    assert_eq!(
        evaluate(defs, &principal(), &grant, target(), 100),
        Ok(caps(&[]))
    );
    let mut defs = definitions();
    defs.scopes[0].capabilities.clear();
    assert_eq!(
        evaluate(defs, &principal(), &credential(), target(), 100),
        Ok(caps(&[]))
    );
    grant.revoked = true;
    assert_eq!(
        evaluate(definitions(), &principal(), &grant, target(), 100),
        Err(Denial::RevokedCredential)
    );
    grant.revoked = false;
    let user = Principal {
        credential_epoch: 3,
        ..principal()
    };
    assert_eq!(
        evaluate(definitions(), &user, &grant, target(), 100),
        Err(Denial::StaleCredential)
    );
}
