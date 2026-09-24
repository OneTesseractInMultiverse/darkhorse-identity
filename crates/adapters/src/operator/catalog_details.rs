use super::{
    authenticated,
    catalog::failure,
    output::{Failure, Output},
};
use crate::password::PasswordPreparation;
use darkhorse_application::{
    operator_accounts,
    registration::{ReadTarget, Record},
};
use darkhorse_domain::{identity::OperationId, operator_accounts::Error};

pub(super) async fn run(target: ReadTarget, stdin: bool) -> Result<Output, Failure> {
    let input = authenticated::credentials(stdin, false).await?;
    if input.reason.is_some() {
        return Err(Failure::usage());
    }
    let id = super::operation_id()?;
    perform(id, target, &input)
        .await
        .map_err(|error| failure(error, id))
}
async fn perform(
    id: OperationId,
    target: ReadTarget,
    input: &authenticated::Input,
) -> Result<Output, Error> {
    let context = authenticated::connect().await?;
    let result = operator_accounts::run(
        &context.store.operator_catalog_details(),
        &context.admission,
        &PasswordPreparation::default(),
        id,
        target,
        &input.email,
        &input.password,
    )
    .await;
    context.store.close().await;
    output(id, target, result?)
}
fn output(operation: OperationId, target: ReadTarget, record: Record) -> Result<Output, Error> {
    Ok(Output::record(
        serde_json::json!({"operation_id":id(operation.as_u128()),"record":project(target,record)?}),
    ))
}
fn project(target: ReadTarget, record: Record) -> Result<serde_json::Value, Error> {
    Ok(match (target, record) {
        (ReadTarget::Application(expected), Record::Application(app)) if app.id == expected => {
            serde_json::json!({"kind":"application","id":id(app.id.as_u128()),"name":app.name,"owner_id":id(app.owner.as_u128()),"owner_email":app.owner_email,"active":app.active,"revision":app.revision.to_string()})
        }
        (
            ReadTarget::Client {
                application,
                client,
            },
            Record::Client(record),
        ) if record.application == application && record.id == client => {
            serde_json::json!({"kind":"client","id":id(client.as_u128()),"application_id":id(application.as_u128()),"name":record.spec.name.as_str(),"active":record.spec.active,"revision":record.revision.to_string(),"token_endpoint_auth_method":"client_secret_basic","refresh_tokens":record.spec.refresh_tokens,"redirect_uris":record.spec.redirects.values(),"resource_ids":record.spec.resources.iter().map(|r|id(r.as_u128())).collect::<Vec<_>>(),"scope_ids":record.spec.scopes.iter().map(|s|id(s.as_u128())).collect::<Vec<_>>()})
        }
        _ => return Err(Error::Unavailable),
    })
}
fn id(value: u128) -> String {
    uuid::Uuid::from_u128(value).to_string()
}
#[cfg(test)]
#[path = "../../tests/unit/operator/catalog_details.rs"]
mod tests;
