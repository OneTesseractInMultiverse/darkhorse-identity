use super::*;
use darkhorse_application::registration::{Record, Written as RegistrationWritten};
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
