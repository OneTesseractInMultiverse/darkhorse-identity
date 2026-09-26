use super::*;
use darkhorse_application::registration::{Record, Written as RegistrationWritten};
use darkhorse_domain::policy_map::{self, NodeId, Relationship};
pub(super) fn registration(record: Record) -> Value {
    let mut value = crate::registration_http::output::record(record);
    if let Some(n) = value.get("revision").and_then(Value::as_u64) {
        value["revision"] = n.to_string().into();
    }
    value
}
pub(super) fn registered(written: RegistrationWritten) -> Value {
    let mut value = json!({"record":registration(written.record)});
    if let Some(secret) = written.secret {
        value["client_secret"] = secret.into();
    }
    value
}
fn reference(id: u128) -> String {
    uuid::Uuid::from_u128(id).to_string()
}
pub(super) fn item(value: Item) -> Value {
    match value {
        Item::Application(v) => registration(Record::Application(v)),
        Item::Resource(v) => registration(Record::Resource(v)),
        Item::Scope(v) => registration(Record::Scope(v)),
        Item::Client(v) => {
            json!({"kind":"client","id":reference(v.id.as_u128()),"application_id":reference(v.application.as_u128()),"name":v.name,"active":v.active,"revision":v.revision.to_string()})
        }
        Item::Capability(v) => {
            json!({"kind":"capability","id":reference(v.id.as_u128()),"name":v.key,"meaning":v.meaning,"active":!v.retired})
        }
        Item::Role(v) => json!({"kind":"role","id":reference(v.id.as_u128()),"name":v.name}),
    }
}
pub(super) fn page(v: Page) -> Value {
    json!({"items":v.items.into_iter().map(item).collect::<Vec<_>>(),"next":v.next.map(|n|reference(n.get())),"policy_revision":v.policy_revision.to_string()})
}
pub(super) fn view(v: View) -> Value {
    json!({"item":item(v.item),"applications":v.applications.into_iter().map(|a|item(Item::Application(a))).collect::<Vec<_>>(),"capabilities":v.capabilities.into_iter().map(|c|item(Item::Capability(c))).collect::<Vec<_>>(),"policy_revision":v.policy_revision.to_string()})
}

pub(super) fn policy_map(graph: policy_map::Graph) -> Value {
    let application_id = reference(graph.application.id.as_u128());
    let application_node = NodeId::Application(graph.application.id);
    let mut nodes = vec![json!({
        "id": node_id(application_node),
        "type": "application",
        "identifier": application_id,
        "name": graph.application.name.as_str(),
        "active": graph.application.active
    })];
    nodes.extend(graph.roles.into_iter().map(|role| {
        json!({
            "id": node_id(NodeId::Role(role.id)),
            "type": "role",
            "identifier": reference(role.id.as_u128()),
            "name": role.name.as_str()
        })
    }));
    nodes.extend(graph.capabilities.into_iter().map(|capability| {
        json!({
            "id": node_id(NodeId::Capability(capability.id)),
            "type": "capability",
            "identifier": reference(capability.id.as_u128()),
            "name": capability.definition.key(),
            "key": capability.definition.key(),
            "meaning": capability.definition.meaning(),
            "retired": capability.retired
        })
    }));
    nodes.extend(graph.resources.into_iter().map(|resource| {
        json!({
            "id": node_id(NodeId::Resource(resource.id)),
            "type": "resource",
            "identifier": reference(resource.id.as_u128()),
            "name": resource.name.as_str(),
            "audience": resource.audience
        })
    }));
    nodes.extend(graph.scopes.into_iter().map(|scope| {
        json!({
            "id": node_id(NodeId::Scope(scope.id)),
            "type": "scope",
            "identifier": reference(scope.id.as_u128()),
            "name": scope.name.as_str(),
            "resource_id": node_id(NodeId::Resource(scope.resource))
        })
    }));
    let edges = graph
        .edges
        .into_iter()
        .map(|edge| {
            let source = node_id(edge.from);
            let target = node_id(edge.to);
            let relationship = relationship(edge.relationship);
            json!({
                "id": format!("edge:{source}:{relationship}:{target}"),
                "source": source,
                "target": target,
                "relationship": relationship
            })
        })
        .collect::<Vec<_>>();
    json!({
        "application": {
            "id": application_id,
            "name": graph.application.name.as_str(),
            "active": graph.application.active
        },
        "policy_revision": graph.policy_revision.to_string(),
        "complete": true,
        "nodes": nodes,
        "edges": edges
    })
}

fn node_id(value: NodeId) -> String {
    match value {
        NodeId::Application(id) => format!("application:{}", reference(id.as_u128())),
        NodeId::Role(id) => format!("role:{}", reference(id.as_u128())),
        NodeId::Capability(id) => format!("capability:{}", reference(id.as_u128())),
        NodeId::Resource(id) => format!("resource:{}", reference(id.as_u128())),
        NodeId::Scope(id) => format!("scope:{}", reference(id.as_u128())),
    }
}

fn relationship(value: Relationship) -> &'static str {
    match value {
        Relationship::ApplicationRole => "application_role",
        Relationship::ApplicationCapability => "application_capability",
        Relationship::ApplicationResource => "application_resource",
        Relationship::RoleCapability => "role_capability",
        Relationship::ResourceCapability => "resource_capability",
        Relationship::ResourceScope => "resource_scope",
        Relationship::ScopeCapability => "scope_capability",
    }
}
pub(super) fn written(v: Written) -> Value {
    json!({"target":target(v.target),"policy_revision":v.policy_revision.to_string()})
}
fn target(v: Target) -> Value {
    match v {
        Target::Capability(id) => json!({"kind":"capabilities","id":reference(id.as_u128())}),
        Target::Role(id) => json!({"kind":"roles","id":reference(id.as_u128())}),
        Target::Resource(app, id) => {
            json!({"kind":"resources","id":reference(id.as_u128()),"application_id":reference(app.as_u128())})
        }
        Target::Scope(app, res, id) => {
            json!({"kind":"scopes","id":reference(id.as_u128()),"application_id":reference(app.as_u128()),"resource_id":reference(res.as_u128())})
        }
    }
}
