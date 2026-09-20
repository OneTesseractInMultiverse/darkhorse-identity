use darkhorse_application::personal_keys::*;
use serde_json::{Value, json};
fn id(n: u128) -> String {
    uuid::Uuid::from_u128(n).to_string()
}
pub(super) fn record(record: Record) -> Value {
    json!({"id":id(record.id.as_u128()),"name":record.name,"application_id":id(record.application.as_u128()),"created_ms":record.created_ms,"expires_ms":record.expires_ms,"active":record.active,"grants":record.grants.into_iter().map(|g|json!({"resource_id":id(g.resource.as_u128()),"capabilities":g.ceiling.iter().map(|c|id(c.as_u128())).collect::<Vec<_>>()})).collect::<Vec<_>>()})
}
pub(super) fn page(page: Page) -> Value {
    json!({"items":page.items.into_iter().map(record).collect::<Vec<_>>(),"next":page.next.map(|k|id(k.as_u128()))})
}
pub(super) fn options(options: Options) -> Value {
    json!({"policy_revision":options.revision.to_string(),"policy":{"default_days":options.policy.default_days(),"maximum_days":options.policy.maximum_days(),"allow_never":options.policy.allow_never()},"items":options.items.into_iter().map(|r|json!({"application_id":id(r.application.as_u128()),"application_name":r.application_name,"resource_id":id(r.resource.as_u128()),"resource_name":r.resource_name,"capabilities":r.capabilities.into_iter().map(|c|json!({"id":id(c.id.as_u128()),"key":c.key,"meaning":c.meaning})).collect::<Vec<_>>()})).collect::<Vec<_>>(),"next":options.next.map(|r|id(r.as_u128()))})
}
