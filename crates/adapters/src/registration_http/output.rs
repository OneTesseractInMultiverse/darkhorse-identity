use super::*;
pub(super) fn written(written: Written) -> Value {
    let mut response = json!({"record":record(written.record)});
    if let Some(secret) = written.secret {
        response["client_secret"] = Value::String(secret);
    }
    response
}
pub(super) fn record(record: Record) -> Value {
    match record {
        Record::Application(r) => {
            json!({"kind":"application","id":id(r.id.as_u128()),"name":r.name,"owner_id":id(r.owner.as_u128()),"owner_email":r.owner_email,"active":r.active,"revision":r.revision})
        }
        Record::Resource(r) => {
            json!({"kind":"resource","id":id(r.id.as_u128()),"application_id":id(r.application.as_u128()),"name":r.name,"audience":r.audience})
        }
        Record::Scope(r) => {
            json!({"kind":"scope","id":id(r.id.as_u128()),"application_id":id(r.application.as_u128()),"resource_id":id(r.resource.as_u128()),"name":r.name})
        }
        Record::Client(r) => client(r),
    }
}
fn client(r: ClientRecord) -> Value {
    json!({"kind":"client","id":id(r.id.as_u128()),"application_id":id(r.application.as_u128()),"name":r.spec.name.as_str(),"active":r.spec.active,"revision":r.revision,
        "token_endpoint_auth_method":"client_secret_basic","redirect_uris":r.spec.redirects.values(),
        "resource_ids":r.spec.resources.iter().map(|r|id(r.as_u128())).collect::<Vec<_>>(),
        "scope_ids":r.spec.scopes.iter().map(|s|id(s.as_u128())).collect::<Vec<_>>(),
        "secrets":r.secrets.into_iter().map(|s|json!({"id":id(s.id.as_u128()),"created_ms":s.created_ms,"expires_ms":s.expires_ms})).collect::<Vec<_>>()})
}
fn id(value: u128) -> String {
    uuid::Uuid::from_u128(value).to_string()
}
