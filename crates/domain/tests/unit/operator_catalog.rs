use super::*;
#[test]
fn catalog_pages_preserve_typed_target_and_validate_all_query_bounds() {
    let app = ApplicationId::from_u128(7).unwrap();
    let query = Query {
        search: String::new(),
        active: None,
        after: None,
        limit: 25,
    };
    for target in [Target::Applications, Target::Clients(app)] {
        for limit in [1, 25] {
            let r = Request::new(
                target,
                Query {
                    limit,
                    ..query.clone()
                },
            )
            .unwrap();
            assert_eq!(r.target(), target);
            assert_eq!(r.query().limit, limit);
        }
    }
    for limit in [0, 26, 100, u16::MAX] {
        assert_eq!(
            Request::new(
                Target::Applications,
                Query {
                    limit,
                    ..query.clone()
                }
            ),
            Err(Error::Invalid)
        );
    }
    for search in ["x\0y".into(), " trim".into(), "界".repeat(101)] {
        assert_eq!(
            Request::new(
                Target::Applications,
                Query {
                    search,
                    ..query.clone()
                }
            ),
            Err(Error::Invalid)
        );
    }
    assert!(
        Request::new(
            Target::Applications,
            Query {
                search: "界".repeat(100),
                ..query
            }
        )
        .is_ok()
    );
}

#[test]
fn access_catalog_selection_and_status_are_explicit() {
    let app = ApplicationId::from_u128(7).unwrap();
    for target in [
        Target::Resources(app),
        Target::Scopes(app),
        Target::Roles(Definitions::Application(app)),
        Target::Roles(Definitions::All),
        Target::Capabilities(Definitions::Application(app)),
        Target::Capabilities(Definitions::All),
    ] {
        for active in [None, Some(true), Some(false)] {
            let query = Query {
                search: "audit%_\\marker".into(),
                active,
                after: None,
                limit: 25,
            };
            let expected = active.is_none() || matches!(target, Target::Capabilities(_));
            assert_eq!(Request::new(target, query.clone()).is_ok(), expected);
            if expected {
                assert_eq!(Request::new(target, query.clone()).unwrap().query(), &query);
            }
        }
    }
}

#[test]
fn catalog_detail_requests_keep_typed_targets_and_explicit_definition_scope() {
    use crate::identity::{CapabilityId, ResourceId, RoleId, ScopeId};

    let application = ApplicationId::from_u128(7).unwrap();
    let resource = ResourceId::from_u128(8).unwrap();
    let scope = ScopeId::from_u128(9).unwrap();
    let role = RoleId::from_u128(10).unwrap();
    let capability = CapabilityId::from_u128(11).unwrap();

    for target in [
        ViewTarget::Resource {
            application,
            id: resource,
        },
        ViewTarget::Scope {
            application,
            resource,
            id: scope,
        },
        ViewTarget::Role {
            id: role,
            selection: Definitions::Application(application),
        },
        ViewTarget::Role {
            id: role,
            selection: Definitions::All,
        },
        ViewTarget::Capability {
            id: capability,
            selection: Definitions::Application(application),
        },
        ViewTarget::Capability {
            id: capability,
            selection: Definitions::All,
        },
    ] {
        assert_eq!(ViewRequest::new(target).target(), target);
    }
}

#[test]
fn access_catalog_mutation_requires_a_bounded_revision_and_reason() {
    use crate::{
        admin_catalog::Change,
        identity::{CapabilityId, PrincipalId, RoleId},
    };

    let change = Change::RetireCapability(CapabilityId::from_u128(11).unwrap());
    let request = MutationRequest::new(9, change.clone(), "Remove deprecated permission").unwrap();
    assert_eq!(request.policy_revision(), 9);
    assert_eq!(request.reason(), "Remove deprecated permission");
    assert!(matches!(request.change(), Change::RetireCapability(_)));

    let too_long = "x".repeat(513);
    for invalid in ["", " ", "line\nbreak", too_long.as_str()] {
        assert_eq!(
            MutationRequest::new(
                9,
                Change::RetireCapability(CapabilityId::from_u128(11).unwrap()),
                invalid,
            )
            .err(),
            Some(Error::Invalid)
        );
    }
    assert_eq!(
        MutationRequest::new(i64::MAX as u64 + 1, change, "Bounded reason").err(),
        Some(Error::Invalid)
    );
    assert!(
        MutationRequest::new(
            9,
            Change::PrincipalRole {
                principal: PrincipalId::from_u128(1).unwrap(),
                application: ApplicationId::from_u128(2).unwrap(),
                role: RoleId::from_u128(3).unwrap(),
                assigned: true,
                principal_revision: i64::MAX as u64,
            },
            "Grant the user role",
        )
        .is_ok()
    );
    assert_eq!(
        MutationRequest::new(
            9,
            Change::PrincipalRole {
                principal: PrincipalId::from_u128(1).unwrap(),
                application: ApplicationId::from_u128(2).unwrap(),
                role: RoleId::from_u128(3).unwrap(),
                assigned: true,
                principal_revision: i64::MAX as u64 + 1,
            },
            "Grant the user role",
        )
        .err(),
        Some(Error::Invalid)
    );
}
