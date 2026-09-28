use super::*;
use crate::operator::output::{Format, OUTPUT_LIMIT, render};
use darkhorse_application::{
    admin_catalog::{CapabilitySummary, Item, RoleSummary, View},
    registration::{ApplicationRecord, ResourceRecord, ScopeRecord},
};
use darkhorse_domain::{
    identity::{ApplicationId, CapabilityId, PrincipalId, ResourceId, RoleId, ScopeId},
    operator_catalog::{Definitions, ViewTarget},
};

fn app(id: u128) -> ApplicationRecord {
    ApplicationRecord {
        id: ApplicationId::from_u128(id).unwrap(),
        name: "Application".into(),
        owner: PrincipalId::from_u128(1).unwrap(),
        owner_email: "private-owner@example.com".into(),
        active: true,
        revision: 3,
    }
}

fn capability(id: u128) -> CapabilitySummary {
    CapabilitySummary {
        id: CapabilityId::from_u128(id).unwrap(),
        key: "records.read".into(),
        meaning: "private description".into(),
        retired: false,
    }
}

fn view(item: Item) -> View {
    View {
        item,
        applications: vec![app(2)],
        capabilities: vec![capability(4)],
        policy_revision: i64::MAX as u64,
    }
}

#[test]
fn role_details_return_only_explicitly_scoped_metadata_and_bounded_capability_summaries() {
    let target = ViewTarget::Role {
        id: RoleId::from_u128(3).unwrap(),
        selection: Definitions::Application(ApplicationId::from_u128(2).unwrap()),
    };
    let result = output(
        OperationId::from_u128(1).unwrap(),
        ViewRequest::new(target),
        view(Item::Role(RoleSummary {
            id: RoleId::from_u128(3).unwrap(),
            name: "Auditor".into(),
        })),
    )
    .unwrap();
    assert_eq!(result.data["record"]["definition_scope"], "application");
    assert_eq!(result.data["record"]["item"]["name"], "Auditor");
    assert_eq!(result.data["record"]["applications"][0]["id"], id_string(2));
    assert_eq!(
        result.data["record"]["capabilities"][0]["key"],
        "records.read"
    );
    assert_eq!(
        result.data["record"]["capabilities"][0]
            .as_object()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        result.data["record"]["policy_revision"],
        i64::MAX.to_string()
    );
    assert!(!result.data.to_string().contains("private"));
    assert!(!result.data.to_string().contains("owner@example.com"));
}

#[test]
fn capability_details_include_its_meaning_but_not_secret_or_owner_material() {
    let target = ViewTarget::Capability {
        id: CapabilityId::from_u128(4).unwrap(),
        selection: Definitions::All,
    };
    let result = output(
        OperationId::from_u128(1).unwrap(),
        ViewRequest::new(target),
        view(Item::Capability(capability(4))),
    )
    .unwrap();
    assert_eq!(result.data["record"]["definition_scope"], "all-definitions");
    assert_eq!(
        result.data["record"]["item"]["meaning"],
        "private description"
    );
    assert_eq!(result.data["record"]["item"]["retired"], false);
    assert!(!result.data.to_string().contains("owner@example.com"));
}

#[test]
fn resource_and_scope_details_require_the_expected_application_and_parent() {
    let application = ApplicationId::from_u128(2).unwrap();
    let resource_id = ResourceId::from_u128(5).unwrap();
    let scope_id = ScopeId::from_u128(6).unwrap();
    let resource = ResourceRecord {
        id: resource_id,
        application,
        name: "Records".into(),
        audience: "urn:darkhorse:records".into(),
    };
    let scope = ScopeRecord {
        id: scope_id,
        application,
        resource: resource_id,
        name: "records:read".into(),
    };
    assert!(
        project(
            ViewTarget::Resource {
                application,
                id: resource_id,
            },
            view(Item::Resource(resource.clone()))
        )
        .is_ok()
    );
    assert!(
        project(
            ViewTarget::Scope {
                application,
                resource: resource_id,
                id: scope_id,
            },
            view(Item::Scope(scope.clone()))
        )
        .is_ok()
    );
    assert!(
        project(
            ViewTarget::Resource {
                application: ApplicationId::from_u128(9).unwrap(),
                id: resource_id,
            },
            view(Item::Resource(resource))
        )
        .is_err()
    );
    assert!(
        project(
            ViewTarget::Scope {
                application,
                resource: ResourceId::from_u128(10).unwrap(),
                id: scope_id,
            },
            view(Item::Scope(scope))
        )
        .is_err()
    );
}

#[test]
fn detail_output_rejects_oversized_relationships_and_escapes_terminal_controls() {
    let target = ViewTarget::Role {
        id: RoleId::from_u128(3).unwrap(),
        selection: Definitions::All,
    };
    let mut oversized = view(Item::Role(RoleSummary {
        id: RoleId::from_u128(3).unwrap(),
        name: "Auditor".into(),
    }));
    oversized.applications = (2..=27).map(app).collect();
    assert!(matches!(
        output(
            OperationId::from_u128(1).unwrap(),
            ViewRequest::new(target),
            oversized,
        ),
        Err(Error::Unavailable)
    ));

    let mut unsafe_view = view(Item::Role(RoleSummary {
        id: RoleId::from_u128(3).unwrap(),
        name: "Auditor\u{202e}".into(),
    }));
    unsafe_view.capabilities.clear();
    let result = output(
        OperationId::from_u128(1).unwrap(),
        ViewRequest::new(target),
        unsafe_view,
    )
    .unwrap();
    let bytes = render(&result, Format::Human).unwrap();
    assert!(bytes.len() < OUTPUT_LIMIT);
    assert!(!String::from_utf8(bytes).unwrap().contains('\u{202e}'));
}
