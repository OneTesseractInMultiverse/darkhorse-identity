use super::*;
use crate::{
    admin_catalog::PermissionDefinition,
    identity::*,
    registration::{Label, ScopeName},
};

fn label(value: &str) -> Label {
    Label::new(value).unwrap()
}

fn scope_name(value: &str) -> ScopeName {
    ScopeName::new(value).unwrap()
}

fn snapshot() -> Snapshot {
    let application = ApplicationId::from_u128(1).unwrap();
    let first_resource = ResourceId::from_u128(6).unwrap();
    let empty_resource = ResourceId::from_u128(7).unwrap();
    let active_capability = CapabilityId::from_u128(4).unwrap();
    let secondary_capability = CapabilityId::from_u128(5).unwrap();
    let retired_capability = CapabilityId::from_u128(11).unwrap();
    let first_scope = ScopeId::from_u128(8).unwrap();
    let empty_scope = ScopeId::from_u128(9).unwrap();

    Snapshot {
        application: Application {
            id: application,
            name: label("Inventory"),
            active: true,
        },
        policy_revision: 42,
        roles: vec![
            Role {
                id: RoleId::from_u128(3).unwrap(),
                name: label("Writer"),
            },
            Role {
                id: RoleId::from_u128(2).unwrap(),
                name: label("Empty role"),
            },
        ],
        capabilities: vec![
            Capability {
                id: secondary_capability,
                definition: PermissionDefinition::new("inventory.read", "Read records").unwrap(),
                retired: false,
            },
            Capability {
                id: retired_capability,
                definition: PermissionDefinition::new("inventory.legacy", "Retired grant").unwrap(),
                retired: true,
            },
            Capability {
                id: active_capability,
                definition: PermissionDefinition::new("inventory.write", "Write records").unwrap(),
                retired: false,
            },
        ],
        resources: vec![
            Resource {
                id: empty_resource,
                application,
                name: label("Unconfigured API"),
                audience: "urn:darkhorse:resource:00000000-0000-0000-0000-000000000007".into(),
            },
            Resource {
                id: first_resource,
                application,
                name: label("Inventory API"),
                audience: "urn:darkhorse:resource:00000000-0000-0000-0000-000000000006".into(),
            },
        ],
        scopes: vec![
            Scope {
                id: empty_scope,
                application,
                resource: empty_resource,
                name: scope_name("inspect"),
            },
            Scope {
                id: first_scope,
                application,
                resource: first_resource,
                name: scope_name("operate"),
            },
        ],
        role_capabilities: vec![(RoleId::from_u128(3).unwrap(), active_capability)],
        resource_capabilities: vec![(first_resource, active_capability)],
        scope_capabilities: vec![(first_scope, secondary_capability)],
    }
}

#[test]
fn projection_contains_only_explicit_application_bindings_and_preserves_revision() {
    let graph = project(snapshot()).unwrap();
    let app = NodeId::Application(ApplicationId::from_u128(1).unwrap());
    let first_role = NodeId::Role(RoleId::from_u128(3).unwrap());
    let empty_role = NodeId::Role(RoleId::from_u128(2).unwrap());
    let write = NodeId::Capability(CapabilityId::from_u128(4).unwrap());
    let read = NodeId::Capability(CapabilityId::from_u128(5).unwrap());
    let retired = NodeId::Capability(CapabilityId::from_u128(11).unwrap());
    let api = NodeId::Resource(ResourceId::from_u128(6).unwrap());
    let empty_api = NodeId::Resource(ResourceId::from_u128(7).unwrap());
    let operate = NodeId::Scope(ScopeId::from_u128(8).unwrap());
    let inspect = NodeId::Scope(ScopeId::from_u128(9).unwrap());

    assert_eq!(graph.policy_revision, 42);
    assert!(graph.application.active);
    assert_eq!(
        graph.roles.iter().map(|node| node.id).collect::<Vec<_>>(),
        [RoleId::from_u128(2).unwrap(), RoleId::from_u128(3).unwrap()]
    );
    assert_eq!(
        graph
            .resources
            .iter()
            .find(|node| node.id == ResourceId::from_u128(6).unwrap())
            .unwrap()
            .audience,
        "urn:darkhorse:resource:00000000-0000-0000-0000-000000000006"
    );
    assert_eq!(
        graph.edges,
        vec![
            Edge {
                from: app,
                relationship: Relationship::ApplicationRole,
                to: empty_role
            },
            Edge {
                from: app,
                relationship: Relationship::ApplicationRole,
                to: first_role
            },
            Edge {
                from: app,
                relationship: Relationship::ApplicationCapability,
                to: write
            },
            Edge {
                from: app,
                relationship: Relationship::ApplicationCapability,
                to: read
            },
            Edge {
                from: app,
                relationship: Relationship::ApplicationCapability,
                to: retired
            },
            Edge {
                from: app,
                relationship: Relationship::ApplicationResource,
                to: api
            },
            Edge {
                from: app,
                relationship: Relationship::ApplicationResource,
                to: empty_api
            },
            Edge {
                from: first_role,
                relationship: Relationship::RoleCapability,
                to: write
            },
            Edge {
                from: api,
                relationship: Relationship::ResourceCapability,
                to: write
            },
            Edge {
                from: api,
                relationship: Relationship::ResourceScope,
                to: operate
            },
            Edge {
                from: empty_api,
                relationship: Relationship::ResourceScope,
                to: inspect
            },
            Edge {
                from: operate,
                relationship: Relationship::ScopeCapability,
                to: read
            },
        ]
    );
    assert!(
        !graph
            .edges
            .iter()
            .any(|edge| edge.from == first_role && edge.to == operate)
    );
    assert!(
        graph
            .capabilities
            .iter()
            .any(|node| node.id == CapabilityId::from_u128(11).unwrap() && node.retired)
    );
    assert!(!graph.edges.iter().any(|edge| {
        edge.to == retired
            && matches!(
                edge.relationship,
                Relationship::RoleCapability
                    | Relationship::ResourceCapability
                    | Relationship::ScopeCapability
            )
    }));
}

#[test]
fn projection_order_is_stable_when_source_rows_arrive_in_a_different_order() {
    let mut source = snapshot();
    let first = project(source.clone()).unwrap();
    source.roles.reverse();
    source.capabilities.reverse();
    source.resources.reverse();
    source.scopes.reverse();
    source.role_capabilities.reverse();
    source.resource_capabilities.reverse();
    source.scope_capabilities.reverse();
    assert_eq!(project(source).unwrap().edges, first.edges);
}

#[test]
fn projection_type_qualifies_identifiers_and_preserves_a_valid_empty_inactive_application() {
    let mut source = snapshot();
    source.roles[0].name = label("Shared name");
    source.roles[1].name = label("Shared name");
    let graph = project(source).unwrap();
    assert_eq!(graph.roles.len(), 2);
    assert_ne!(graph.roles[0].id, graph.roles[1].id);

    let mut source = snapshot();
    source.application.active = false;
    source.roles.clear();
    source.capabilities.clear();
    source.resources.clear();
    source.scopes.clear();
    source.role_capabilities.clear();
    source.resource_capabilities.clear();
    source.scope_capabilities.clear();
    let graph = project(source).unwrap();
    assert!(!graph.application.active);
    assert_eq!(graph.policy_revision, 42);
    assert!(graph.roles.is_empty());
    assert!(graph.capabilities.is_empty());
    assert!(graph.resources.is_empty());
    assert!(graph.scopes.is_empty());
    assert!(graph.edges.is_empty());

    let raw_id = 700;
    assert_ne!(
        NodeId::Role(RoleId::from_u128(raw_id).unwrap()),
        NodeId::Capability(CapabilityId::from_u128(raw_id).unwrap())
    );
}

#[test]
fn projection_rejects_duplicate_nodes_relationships_and_missing_references() {
    let mut source = snapshot();
    source.roles.push(source.roles[0].clone());
    assert_eq!(project(source).unwrap_err(), Error::InvalidSnapshot);

    let mut source = snapshot();
    source.capabilities.push(source.capabilities[0].clone());
    assert_eq!(project(source).unwrap_err(), Error::InvalidSnapshot);

    let mut source = snapshot();
    source.resources.push(source.resources[0].clone());
    assert_eq!(project(source).unwrap_err(), Error::InvalidSnapshot);

    let mut source = snapshot();
    source.scopes.push(source.scopes[0].clone());
    assert_eq!(project(source).unwrap_err(), Error::InvalidSnapshot);

    let mut source = snapshot();
    source.role_capabilities.push(source.role_capabilities[0]);
    assert_eq!(project(source).unwrap_err(), Error::InvalidSnapshot);

    let mut source = snapshot();
    source
        .resource_capabilities
        .push(source.resource_capabilities[0]);
    assert_eq!(project(source).unwrap_err(), Error::InvalidSnapshot);

    let mut source = snapshot();
    source.scope_capabilities.push(source.scope_capabilities[0]);
    assert_eq!(project(source).unwrap_err(), Error::InvalidSnapshot);

    let mut source = snapshot();
    source.role_capabilities[0].1 = CapabilityId::from_u128(99).unwrap();
    assert_eq!(project(source).unwrap_err(), Error::InvalidSnapshot);

    let mut source = snapshot();
    source.resource_capabilities[0].1 = CapabilityId::from_u128(99).unwrap();
    assert_eq!(project(source).unwrap_err(), Error::InvalidSnapshot);

    let mut source = snapshot();
    source.scope_capabilities[0].1 = CapabilityId::from_u128(99).unwrap();
    assert_eq!(project(source).unwrap_err(), Error::InvalidSnapshot);

    let mut source = snapshot();
    source.scope_capabilities[0].0 = ScopeId::from_u128(99).unwrap();
    assert_eq!(project(source).unwrap_err(), Error::InvalidSnapshot);
}

#[test]
fn projection_rejects_cross_application_resources_and_orphaned_scopes() {
    let mut source = snapshot();
    source.resources[0].application = ApplicationId::from_u128(99).unwrap();
    assert_eq!(project(source).unwrap_err(), Error::InvalidSnapshot);

    let mut source = snapshot();
    source.scopes[0].application = ApplicationId::from_u128(99).unwrap();
    assert_eq!(project(source).unwrap_err(), Error::InvalidSnapshot);

    let mut source = snapshot();
    source.scopes[0].resource = ResourceId::from_u128(99).unwrap();
    assert_eq!(project(source).unwrap_err(), Error::InvalidSnapshot);
}

#[test]
fn projection_rejects_grants_to_retired_capabilities_and_preserves_labels_as_text() {
    let mut source = snapshot();
    source.role_capabilities[0].1 = CapabilityId::from_u128(11).unwrap();
    assert_eq!(project(source).unwrap_err(), Error::InvalidSnapshot);

    let mut source = snapshot();
    source.application.name = label("<script>alert(1)</script>");
    let graph = project(source).unwrap();
    assert_eq!(graph.application.name.as_str(), "<script>alert(1)</script>");
}

#[test]
fn projection_fails_instead_of_truncating_oversized_graphs() {
    let mut source = snapshot();
    source.roles = (0..MAX_NODES)
        .map(|index| Role {
            id: RoleId::from_u128(100 + index as u128).unwrap(),
            name: label("Role"),
        })
        .collect();
    assert_eq!(project(source).unwrap_err(), Error::TooLarge);

    let mut source = snapshot();
    source.role_capabilities = vec![
        (
            RoleId::from_u128(3).unwrap(),
            CapabilityId::from_u128(4).unwrap()
        );
        MAX_EDGES + 1
    ];
    assert_eq!(project(source).unwrap_err(), Error::TooLarge);
}

#[test]
fn projection_rejects_revision_outside_the_database_integer_range() {
    let mut source = snapshot();
    source.policy_revision = i64::MAX as u64 + 1;
    assert_eq!(project(source).unwrap_err(), Error::InvalidSnapshot);
}
