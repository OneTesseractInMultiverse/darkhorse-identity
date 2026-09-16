//! Exhaustive small-set properties: no random seed, external files, or services.
use super::{fixtures::*, *};
use std::collections::BTreeSet;

fn mask_set(mask: u8) -> CapabilitySet {
    (0..3)
        .filter(|bit| mask & (1 << bit) != 0)
        .map(|bit| cap(bit + 1))
        .collect()
}

fn catalog_for(roles: u8, resources: u8, scopes: u8) -> Catalog {
    let mut defs = definitions();
    defs.roles[0].capabilities = mask_set(roles);
    defs.resources[0].capabilities = mask_set(resources);
    defs.scopes[0].capabilities = mask_set(scopes & resources);
    defs.scopes[1].capabilities.clear();
    Catalog::new(defs).unwrap()
}

#[test]
fn property_effective_access_is_exact_intersection_for_all_small_sets() {
    let user = principal();
    for roles in 0..8 {
        for resources in 0..8 {
            for scopes in 0..8 {
                let catalog = catalog_for(roles, resources, scopes);
                for ceiling in 0..8 {
                    let grant = CredentialGrant {
                        ceiling: mask_set(ceiling),
                        ..credential()
                    };
                    let input = Evaluation {
                        principal: &user,
                        credential: &grant,
                        target: target(),
                        now: 100,
                    };
                    let effective = effective_capabilities(&catalog, &input).unwrap();
                    assert_eq!(effective, mask_set(roles & resources & scopes & ceiling));
                    for bit in 0..3 {
                        assert_eq!(
                            authorize(&catalog, &input, cap(bit + 1)).is_ok(),
                            roles & resources & scopes & ceiling & (1 << bit) != 0
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn property_removing_role_or_scope_authority_never_increases_access() {
    let user = principal();
    for roles in 0..8 {
        for scopes in 0..8 {
            for ceiling in 0..8 {
                let grant = CredentialGrant {
                    ceiling: mask_set(ceiling),
                    ..credential()
                };
                let input = Evaluation {
                    principal: &user,
                    credential: &grant,
                    target: target(),
                    now: 100,
                };
                let before =
                    effective_capabilities(&catalog_for(roles, 7, scopes), &input).unwrap();
                for removed in 0..8 {
                    for catalog in [
                        catalog_for(roles & !removed, 7, scopes),
                        catalog_for(roles, 7, scopes & !removed),
                    ] {
                        assert!(
                            effective_capabilities(&catalog, &input)
                                .unwrap()
                                .is_subset(&before)
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn property_all_keys_snapshot_access_and_growth_cannot_expand_issued_ceiling() {
    let user = principal();
    for roles in 0..8 {
        for limit in 0..8 {
            let catalog = catalog_for(roles, 7, 7);
            let result = plan_key(
                &catalog,
                &user,
                target(),
                CapabilitySelection::All,
                &mask_set(limit),
            );
            if roles & limit == 0 {
                assert_eq!(result, Err(Denial::EmptyGrant));
                continue;
            }
            let plan = result.unwrap();
            assert_eq!(plan.ceiling, mask_set(roles & limit));
            let grant = CredentialGrant {
                ceiling: plan.ceiling.clone(),
                delegation: plan.delegation,
                ..credential()
            };
            let input = Evaluation {
                principal: &user,
                credential: &grant,
                target: target(),
                now: 100,
            };
            for expanded in 0..8 {
                assert_eq!(
                    effective_capabilities(&catalog_for(roles | expanded, 7, 7), &input),
                    Ok(plan.ceiling.clone())
                );
            }
        }
    }
}

#[test]
fn property_oauth_scope_growth_cannot_expand_the_issuance_ceiling() {
    let user = principal();
    for scopes in 0..8 {
        for consent in 0..8 {
            let selected = BTreeSet::from([scope(1)]);
            let result = plan_oauth(
                &catalog_for(7, 7, scopes),
                &user,
                target(),
                client(1),
                selected,
                &mask_set(consent),
            );
            if scopes & consent == 0 {
                assert_eq!(result, Err(Denial::EmptyGrant));
                continue;
            }
            let plan = result.unwrap();
            assert_eq!(plan.ceiling, mask_set(scopes & consent));
            let grant = CredentialGrant {
                ceiling: plan.ceiling.clone(),
                delegation: plan.delegation,
                ..credential()
            };
            let input = Evaluation {
                principal: &user,
                credential: &grant,
                target: target(),
                now: 100,
            };
            for expanded in 0..8 {
                assert_eq!(
                    effective_capabilities(&catalog_for(7, 7, scopes | expanded), &input),
                    Ok(plan.ceiling.clone())
                );
            }
        }
    }
}

#[test]
fn property_explicit_subsets_and_refresh_attenuation_reject_every_expansion() {
    for available in 0..8 {
        for requested in 0..8 {
            let expected = if requested == 0 {
                Err(Denial::EmptyGrant)
            } else if requested & !available != 0 {
                Err(Denial::DelegationExceedsAccess)
            } else {
                Ok(mask_set(requested))
            };
            assert_eq!(
                attenuate(&mask_set(available), &mask_set(requested)),
                expected
            );
            assert_eq!(
                plan_key(
                    &catalog_for(available, 7, 7),
                    &principal(),
                    target(),
                    CapabilitySelection::Subset(mask_set(requested)),
                    &mask_set(7)
                )
                .map(|plan| plan.ceiling),
                expected
            );
        }
    }
}

#[test]
fn property_shared_role_requires_a_separate_assignment_for_each_application() {
    for mask in 0..8 {
        let mut defs = definitions();
        for capability in &mut defs.capabilities[..3] {
            capability.applications.insert(app(2));
        }
        defs.roles[0].applications.insert(app(2));
        defs.roles[0].capabilities = mask_set(mask);
        defs.resources[1].capabilities.extend(mask_set(7));
        let catalog = Catalog::new(defs).unwrap();
        let user = principal();
        let second = Target {
            application: app(2),
            resource: resource(2),
        };
        let grant = CredentialGrant {
            target: second,
            ceiling: mask_set(7),
            delegation: Delegation::PersonalKey,
            ..credential()
        };
        let input = Evaluation {
            principal: &user,
            credential: &grant,
            target: second,
            now: 100,
        };
        assert_eq!(effective_capabilities(&catalog, &input), Ok(caps(&[])));
        let mut assigned = user.clone();
        assigned.assignments.insert(Assignment {
            application: app(2),
            role: role(1),
        });
        assert_eq!(
            effective_capabilities(
                &catalog,
                &Evaluation {
                    principal: &assigned,
                    ..input
                }
            ),
            Ok(mask_set(mask))
        );
    }
}
