use super::*;
use crate::{AccountStatus, authorization::fixtures::*};

#[test]
fn all_keys_snapshot_delegable_access_and_subset_keys_cannot_expand_it() {
    let catalog = Catalog::new(definitions()).unwrap();
    let user = principal();
    let plan = plan_key(
        &catalog,
        &user,
        target(),
        CapabilitySelection::All,
        &caps(&[1, 3]),
    )
    .unwrap();
    assert_eq!(
        plan,
        IssuancePlan {
            subject: user.id,
            target: target(),
            principal_epoch: 2,
            ceiling: caps(&[1]),
            delegation: Delegation::PersonalKey
        }
    );
    assert_eq!(
        plan_key(
            &catalog,
            &user,
            target(),
            CapabilitySelection::Subset(caps(&[2])),
            &caps(&[1, 2])
        )
        .unwrap()
        .ceiling,
        caps(&[2])
    );
    for selection in [caps(&[3]), caps(&[4]), caps(&[99]), caps(&[1, 2])] {
        assert_eq!(
            plan_key(
                &catalog,
                &user,
                target(),
                CapabilitySelection::Subset(selection),
                &caps(&[1])
            ),
            Err(Denial::DelegationExceedsAccess)
        );
    }
}

#[test]
fn issuance_rejects_empty_authority_and_invalid_principals_and_targets() {
    let catalog = Catalog::new(definitions()).unwrap();
    assert_eq!(
        plan_key(
            &catalog,
            &principal(),
            target(),
            CapabilitySelection::All,
            &caps(&[])
        ),
        Err(Denial::EmptyGrant)
    );
    assert_eq!(
        plan_key(
            &catalog,
            &principal(),
            target(),
            CapabilitySelection::Subset(caps(&[])),
            &caps(&[1])
        ),
        Err(Denial::EmptyGrant)
    );
    let user = Principal {
        status: AccountStatus::Inactive,
        ..principal()
    };
    assert_eq!(
        plan_key(
            &catalog,
            &user,
            target(),
            CapabilitySelection::All,
            &caps(&[1])
        ),
        Err(Denial::InactivePrincipal)
    );
    let other = Target {
        application: app(2),
        resource: resource(1),
    };
    assert_eq!(
        plan_key(
            &catalog,
            &principal(),
            other,
            CapabilitySelection::All,
            &caps(&[1])
        ),
        Err(Denial::AudienceMismatch)
    );
    let mut defs = definitions();
    defs.roles[0].capabilities.clear();
    assert_eq!(
        plan_key(
            &Catalog::new(defs).unwrap(),
            &principal(),
            target(),
            CapabilitySelection::All,
            &caps(&[1])
        ),
        Err(Denial::EmptyGrant)
    );
}

#[test]
fn oauth_plan_intersects_role_scope_and_consent_and_preserves_bindings() {
    let catalog = Catalog::new(definitions()).unwrap();
    let scopes = BTreeSet::from([scope(1), scope(2)]);
    let plan = plan_oauth(
        &catalog,
        &principal(),
        target(),
        client(1),
        scopes.clone(),
        &caps(&[1, 3]),
    )
    .unwrap();
    assert_eq!(
        plan,
        IssuancePlan {
            subject: principal_id(1),
            target: target(),
            principal_epoch: 2,
            ceiling: caps(&[1]),
            delegation: Delegation::OAuth {
                client: client(1),
                scopes
            }
        }
    );
    assert_eq!(
        plan_oauth(
            &catalog,
            &principal(),
            target(),
            client(1),
            BTreeSet::from([scope(1)]),
            &caps(&[2])
        ),
        Err(Denial::EmptyGrant)
    );
    assert_eq!(
        plan_oauth(
            &catalog,
            &principal(),
            target(),
            client(1),
            BTreeSet::from([scope(99)]),
            &caps(&[1])
        ),
        Err(Denial::InvalidScope)
    );
    let user = Principal {
        status: AccountStatus::Inactive,
        ..principal()
    };
    assert_eq!(
        plan_oauth(
            &catalog,
            &user,
            target(),
            client(1),
            BTreeSet::from([scope(1)]),
            &caps(&[1])
        ),
        Err(Denial::InactivePrincipal)
    );
}

#[test]
fn refresh_attenuation_preserves_or_narrows_a_nonempty_ceiling() {
    let original = caps(&[1, 2]);
    assert_eq!(attenuate(&original, &original), Ok(original.clone()));
    assert_eq!(attenuate(&original, &caps(&[2])), Ok(caps(&[2])));
    assert_eq!(
        attenuate(&original, &caps(&[1, 3])),
        Err(Denial::DelegationExceedsAccess)
    );
    assert_eq!(attenuate(&original, &caps(&[])), Err(Denial::EmptyGrant));
    assert_eq!(
        attenuate(&caps(&[]), &caps(&[1])),
        Err(Denial::DelegationExceedsAccess)
    );
    assert_eq!(original, caps(&[1, 2]));
}
