use super::super::{
    authenticated,
    output::{Failure, Output},
};
use crate::{password::PasswordPreparation, registration::OsRegistrationEntropy};
use darkhorse_application::{operator_accounts, registration::ClientRecord};
use darkhorse_domain::{
    identity::{ApplicationId, ClientSecretId, OperationId},
    operator_accounts::Error,
    operator_client_creation::Request,
};
pub(in crate::operator) async fn run(application: ApplicationId) -> Result<Output, Failure> {
    let input = super::input::read(std::io::stdin().lock()).map_err(|_| Failure::usage())?;
    let spec = input.client.complete_spec().map_err(|_| Failure::usage())?;
    let request = Request::new(
        application,
        spec,
        input.authentication.reason.as_deref().unwrap_or(""),
    )
    .map_err(|_| Failure::usage())?;
    let id = super::super::operation_id()?;
    perform(id, request, &input.authentication)
        .await
        .map_err(|error| failure(error, id))
}

async fn perform(
    id: OperationId,
    request: Request,
    input: &authenticated::Input,
) -> Result<Output, Error> {
    let context = authenticated::connect().await?;
    let result = operator_accounts::run(
        &context
            .store
            .operator_client_creation(OsRegistrationEntropy),
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

fn output(
    id: OperationId,
    created: crate::postgres::operator_client_creation::CreatedClient,
) -> Output {
    Output::one_time_secret(data(id, &created.record, created.secret_id), created.secret)
}

fn data(id: OperationId, record: &ClientRecord, secret: ClientSecretId) -> serde_json::Value {
    serde_json::json!({
        "completed": true,
        "operation_id": uuid::Uuid::from_u128(id.as_u128()).to_string(),
        "application_id": uuid::Uuid::from_u128(record.application.as_u128()).to_string(),
        "client_id": uuid::Uuid::from_u128(record.id.as_u128()).to_string(),
        "secret_id": uuid::Uuid::from_u128(secret.as_u128()).to_string(),
        "revision": record.revision.to_string()
    })
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
        Error::NotFound => "Application not found.",
        Error::Conflict => "The client could not be created because current state conflicts.",
        Error::Uncertain => {
            "Outcome unknown; inspect the application and client audit/current state before retrying. A lost secret cannot be recovered; retire it and create a replacement."
        }
        _ => {
            "Client creation unavailable; check configuration, database, limiter and audit availability."
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/operator/client_creation.rs"]
mod tests;
