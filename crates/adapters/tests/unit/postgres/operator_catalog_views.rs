use super::*;
use darkhorse_application::registration::ApplicationRecord;
use darkhorse_domain::identity::{ApplicationId, CapabilityId, ResourceId, RoleId, ScopeId};

#[test]
fn detail_target_mapping_preserves_application_and_parent_identity() {
    let application = ApplicationId::from_u128(1).unwrap();
    let resource = ResourceId::from_u128(2).unwrap();
    let scope = ScopeId::from_u128(3).unwrap();
    let capability = CapabilityId::from_u128(4).unwrap();
    let role = RoleId::from_u128(5).unwrap();

    assert!(matches!(
        catalog_target(ViewTarget::Resource {
            application,
            id: resource,
        }),
        darkhorse_application::admin_catalog::Target::Resource(a, r)
            if a == application && r == resource
    ));
    assert!(matches!(
        catalog_target(ViewTarget::Scope {
            application,
            resource,
            id: scope,
        }),
        darkhorse_application::admin_catalog::Target::Scope(a, r, s)
            if a == application && r == resource && s == scope
    ));
    assert_eq!(
        audit_target(ViewTarget::Role {
            id: role,
            selection: Definitions::All,
        }),
        ("role.show", None, 5, None)
    );
    assert_eq!(
        audit_target(ViewTarget::Capability {
            id: capability,
            selection: Definitions::Application(application),
        }),
        ("capability.show", Some(application), 4, None)
    );
    assert_eq!(
        audit_target(ViewTarget::Scope {
            application,
            resource,
            id: scope,
        }),
        ("scope.show", Some(application), 3, Some(resource))
    );
}

#[test]
fn catalog_detail_not_found_maps_without_disclosing_storage_errors() {
    use darkhorse_domain::registration::RegistrationError;
    assert_eq!(
        registration_error(RegistrationError::NotFound),
        Error::NotFound
    );
    assert_eq!(
        registration_error(RegistrationError::Unavailable),
        Error::Unavailable
    );
}

#[test]
fn scoped_definition_view_keeps_only_selected_application_bindings_and_grants() {
    let application = ApplicationId::from_u128(1).unwrap();
    let foreign_application = ApplicationId::from_u128(2).unwrap();
    let first = CapabilityId::from_u128(4).unwrap();
    let second = CapabilityId::from_u128(5).unwrap();
    let mut view = View {
        item: darkhorse_application::admin_catalog::Item::Role(
            darkhorse_application::admin_catalog::RoleSummary {
                id: RoleId::from_u128(6).unwrap(),
                name: "reader".into(),
            },
        ),
        applications: vec![
            application_record(application),
            application_record(foreign_application),
        ],
        capabilities: vec![capability_summary(first), capability_summary(second)],
        policy_revision: 1,
    };
    let target = ViewTarget::Role {
        id: RoleId::from_u128(6).unwrap(),
        selection: Definitions::Application(application),
    };
    scope_view(target, &mut view, Some(&[Uuid::from_u128(first.as_u128())])).unwrap();
    assert_eq!(view.applications.len(), 1);
    assert_eq!(view.applications[0].id, application);
    assert_eq!(view.capabilities.len(), 1);
    assert_eq!(view.capabilities[0].id, first);
    assert_eq!(
        scope_view(
            target,
            &mut View {
                item: view.item,
                applications: vec![],
                capabilities: vec![],
                policy_revision: 1,
            },
            Some(&[]),
        ),
        Err(Error::NotFound)
    );
}

fn application_record(id: ApplicationId) -> ApplicationRecord {
    ApplicationRecord {
        id,
        name: "application".into(),
        owner: darkhorse_domain::identity::PrincipalId::from_u128(3).unwrap(),
        owner_email: "owner@example.com".into(),
        active: true,
        revision: 0,
    }
}

fn capability_summary(id: CapabilityId) -> darkhorse_application::admin_catalog::CapabilitySummary {
    darkhorse_application::admin_catalog::CapabilitySummary {
        id,
        key: "records.read".into(),
        meaning: "read records".into(),
        retired: false,
    }
}
