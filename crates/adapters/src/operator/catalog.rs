use super::localization::Locale;
use super::{
    authenticated,
    output::{Failure, Output},
};
use crate::password::PasswordPreparation;
use darkhorse_application::{
    admin_catalog::{Item, Page},
    operator_accounts,
};
use darkhorse_domain::{
    identity::OperationId,
    operator_accounts::Error,
    operator_catalog::{Request, Target},
};

pub(super) async fn run(request: Request, stdin: bool, locale: Locale) -> Result<Output, Failure> {
    let input = authenticated::credentials(stdin, false, locale).await?;
    if input.reason.is_some() {
        return Err(Failure::usage());
    }
    let id = super::operation_id()?;
    perform(id, request, &input)
        .await
        .map_err(|error| failure(error, id))
}
async fn perform(
    id: OperationId,
    request: Request,
    input: &authenticated::Input,
) -> Result<Output, Error> {
    let context = authenticated::connect().await?;
    let target = request.target();
    let result = operator_accounts::run(
        &context.store.operator_catalog(),
        &context.admission,
        &PasswordPreparation::default(),
        id,
        request,
        &input.email,
        &input.password,
    )
    .await;
    context.store.close().await;
    output(id, target, result?)
}
fn output(id: OperationId, target: Target, page: Page) -> Result<Output, Error> {
    let items = page
        .items
        .into_iter()
        .map(|item| project(target, item))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Output::record(serde_json::json!({
        "operation_id":uuid::Uuid::from_u128(id.as_u128()).to_string(),
        "items":items,"next":page.next.map(|id|uuid::Uuid::from_u128(id.get()).to_string()),
        "policy_revision":page.policy_revision.to_string()
    })))
}
fn project(target: Target, item: Item) -> Result<serde_json::Value, Error> {
    Ok(match (target, item) {
        (Target::Applications, Item::Application(app)) => serde_json::json!({
            "id":uuid::Uuid::from_u128(app.id.as_u128()).to_string(),"name":app.name,
            "owner_id":uuid::Uuid::from_u128(app.owner.as_u128()).to_string(),"owner_email":app.owner_email,
            "active":app.active,"revision":app.revision.to_string()
        }),
        (Target::Clients(application), Item::Client(client))
            if client.application == application =>
        {
            serde_json::json!({
                "id":uuid::Uuid::from_u128(client.id.as_u128()).to_string(),"application_id":uuid::Uuid::from_u128(client.application.as_u128()).to_string(),
                "name":client.name,"active":client.active,"revision":client.revision.to_string()
            })
        }
        (Target::Resources(application), Item::Resource(resource))
            if resource.application == application =>
        {
            serde_json::json!({
                "id":uuid::Uuid::from_u128(resource.id.as_u128()).to_string(),
                "application_id":uuid::Uuid::from_u128(resource.application.as_u128()).to_string(),
                "name":resource.name,"audience":resource.audience
            })
        }
        (Target::Scopes(application), Item::Scope(scope)) if scope.application == application => {
            serde_json::json!({
                "id":uuid::Uuid::from_u128(scope.id.as_u128()).to_string(),
                "application_id":uuid::Uuid::from_u128(scope.application.as_u128()).to_string(),
                "resource_id":uuid::Uuid::from_u128(scope.resource.as_u128()).to_string(),"name":scope.name
            })
        }
        (Target::Capabilities(_), Item::Capability(capability)) => serde_json::json!({
            "id":uuid::Uuid::from_u128(capability.id.as_u128()).to_string(),
            "key":capability.key,"retired":capability.retired
        }),
        (Target::Roles(_), Item::Role(role)) => serde_json::json!({
            "id":uuid::Uuid::from_u128(role.id.as_u128()).to_string(),"name":role.name
        }),
        _ => return Err(Error::Unavailable),
    })
}
pub(super) fn failure(error: Error, id: OperationId) -> Failure {
    let mut data =
        serde_json::json!({"operation_id":uuid::Uuid::from_u128(id.as_u128()).to_string()});
    if let Error::Limited { retry_after_ms } = error {
        data["retry_after_ms"] = retry_after_ms.into();
    }
    Failure::from(message(error)).with_data(data)
}
fn message(error: Error) -> &'static str {
    match error {
        Error::Denied => "Administrator authentication or authority denied.",
        Error::Limited { .. } => "Authentication attempt limit reached; wait before retrying.",
        Error::NotFound => "Catalog target not found.",
        Error::Uncertain => "Outcome unknown; inspect the catalog read audit before retrying.",
        _ => {
            "Catalog read unavailable; check input, login configuration, database, limiter and audit availability."
        }
    }
}
#[cfg(test)]
#[path = "../../tests/unit/operator/catalog.rs"]
mod tests;
