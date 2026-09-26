use super::localization::Locale;
use super::{
    authenticated,
    output::{Failure, Output},
};
use crate::password::PasswordPreparation;
use darkhorse_application::{operator_accounts, operator_client_secrets::Outcome};
use darkhorse_domain::{
    identity::OperationId,
    operator_accounts::Error,
    operator_client_secrets::{Operation, Request, Target},
};
pub(super) async fn run(
    target: Target,
    operation: Operation,
    stdin: bool,
    locale: Locale,
) -> Result<Output, Failure> {
    let mutation = matches!(operation, Operation::Retire { .. });
    let input = authenticated::credentials(stdin, mutation, locale).await?;
    let request =
        Request::new(target, operation, input.reason.as_deref()).map_err(|_| Failure::usage())?;
    let id = super::operation_id()?;
    perform(id, &request, &input)
        .await
        .map_err(|e| failure(e, id))
}
async fn perform(
    id: OperationId,
    request: &Request,
    input: &authenticated::Input,
) -> Result<Output, Error> {
    let context = authenticated::connect().await?;
    let result = operator_accounts::run(
        &context.store.operator_client_secrets(),
        &context.admission,
        &PasswordPreparation::default(),
        id,
        request.clone(),
        &input.email,
        &input.password,
    )
    .await;
    context.store.close().await;
    output(id, request, result?)
}
fn output(operation: OperationId, request: &Request, outcome: Outcome) -> Result<Output, Error> {
    let target = request.target();
    let mut data = serde_json::json!({
        "operation_id": id(operation.as_u128()),
        "application_id": id(target.application.as_u128()),
        "client_id": id(target.client.as_u128()),
        "revision": outcome.revision().to_string()
    });
    match (request.operation(), outcome) {
        (Operation::List { limit, .. }, Outcome::Listed(page))
            if page.items.len() <= usize::from(limit) =>
        {
            data["observed_ms"] = page.observed_ms.into();
            data["items"] = page
                .items
                .into_iter()
                .map(|item| {
                    serde_json::json!({
                        "id": id(item.id.as_u128()),
                        "created_ms": item.created_ms,
                        "expires_ms": item.expires_ms,
                        "retired": item.retired
                    })
                })
                .collect();
            data["next"] = page.next.map(|s| id(s.as_u128())).into();
        }
        (Operation::Retire { secret, .. }, Outcome::Retired { .. }) => {
            data["completed"] = true.into();
            data["secret_id"] = id(secret.as_u128()).into();
        }
        _ => return Err(Error::Unavailable),
    }
    Ok(Output::record(data))
}
fn id(value: u128) -> String {
    uuid::Uuid::from_u128(value).to_string()
}
fn failure(error: Error, operation: OperationId) -> Failure {
    let mut data = serde_json::json!({"operation_id":id(operation.as_u128())});
    if let Error::Limited { retry_after_ms } = error {
        data["retry_after_ms"] = retry_after_ms.into();
    }
    Failure::from(message(error)).with_data(data)
}
fn message(error: Error) -> &'static str {
    match error {
        Error::Invalid => "Invalid client secret operation.",
        Error::Denied => "Administrator authentication or authority denied.",
        Error::Limited { .. } => "Authentication attempt limit reached; wait before retrying.",
        Error::NotFound => "Client or unretired secret not found in the requested scope.",
        Error::Conflict => "The client changed; read its current revision before retrying.",
        Error::Uncertain => {
            "Outcome unknown; inspect the client secret audit and current inventory before deciding to retry."
        }
        _ => {
            "Client secret operation unavailable; check configuration, database, limiter and audit availability."
        }
    }
}
#[cfg(test)]
#[path = "../../tests/unit/operator/client_secrets.rs"]
mod tests;
