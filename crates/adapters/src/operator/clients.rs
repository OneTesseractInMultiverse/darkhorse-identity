use super::{
    authenticated,
    output::{Failure, Output},
};
use crate::password::PasswordPreparation;
use darkhorse_application::{operator_accounts, registration::ClientRecord};
use darkhorse_domain::{
    identity::{ApplicationId, ClientId, OperationId},
    operator_accounts::Error,
    operator_clients::{Request, Update},
};
mod input;
pub(super) async fn run(
    application: ApplicationId,
    client: ClientId,
    revision: u64,
) -> Result<Output, Failure> {
    let input = input::read(std::io::stdin().lock()).map_err(|_| Failure::usage())?;
    let spec = input.client.complete_spec().map_err(|_| Failure::usage())?;
    let request = Request::new(
        Update {
            application,
            client,
            revision,
            spec,
        },
        input.authentication.reason.as_deref().unwrap_or(""),
    )
    .map_err(|_| Failure::usage())?;
    let id = super::operation_id()?;
    perform(id, request, &input.authentication)
        .await
        .map_err(|e| failure(e, id))
}
async fn perform(
    id: OperationId,
    request: Request,
    input: &authenticated::Input,
) -> Result<Output, Error> {
    let context = authenticated::connect().await?;
    let result = operator_accounts::run(
        &context.store.operator_clients(),
        &context.admission,
        &PasswordPreparation::default(),
        id,
        request,
        &input.email,
        &input.password,
    )
    .await;
    context.store.close().await;
    Ok(output(id, result?))
}
fn output(id: OperationId, record: ClientRecord) -> Output {
    Output::record(
        serde_json::json!({"completed":true,"operation_id":uuid::Uuid::from_u128(id.as_u128()).to_string(),"application_id":uuid::Uuid::from_u128(record.application.as_u128()).to_string(),"client_id":uuid::Uuid::from_u128(record.id.as_u128()).to_string(),"revision":record.revision.to_string()}),
    )
}
fn failure(error: Error, id: OperationId) -> Failure {
    let mut data =
        serde_json::json!({"operation_id":uuid::Uuid::from_u128(id.as_u128()).to_string()});
    if let Error::Limited { retry_after_ms } = error {
        data["retry_after_ms"] = retry_after_ms.into();
    }
    Failure::from(message(error)).with_data(data)
}
fn message(error: Error) -> &'static str {
    match error {
        Error::Invalid => "Invalid client configuration or resource/scope allowance.",
        Error::Denied => "Administrator authentication or authority denied.",
        Error::Limited { .. } => "Authentication attempt limit reached; wait before retrying.",
        Error::NotFound => "Client not found in the requested application.",
        Error::Conflict => "The client changed; read its current revision before retrying.",
        Error::Uncertain => {
            "Outcome unknown; inspect the client mutation audit and current configuration before deciding to retry."
        }
        _ => {
            "Client operation unavailable; check configuration, database, limiter and audit availability."
        }
    }
}
#[cfg(test)]
#[path = "../../tests/unit/operator/clients.rs"]
mod tests;
