use super::{
    authenticated,
    output::{Failure, Output},
};
use crate::{password::PasswordPreparation, registration::OsRegistrationEntropy};
use darkhorse_application::{operator_accounts, registration::ApplicationRecord};
use darkhorse_domain::{
    identity::OperationId,
    operator_accounts::Error,
    operator_applications::{Operation, Request},
};

pub(super) async fn run(operation: Operation, stdin: bool) -> Result<Output, Failure> {
    let input = authenticated::credentials(stdin, true).await?;
    let request = Request::new(operation, input.reason.as_deref().unwrap_or(""))
        .map_err(|_| Failure::usage())?;
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
    let result = operator_accounts::run(
        &context.store.operator_applications(OsRegistrationEntropy),
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
fn output(id: OperationId, record: ApplicationRecord) -> Output {
    Output::record(
        serde_json::json!({"completed":true,"operation_id":uuid::Uuid::from_u128(id.as_u128()).to_string(),"application_id":uuid::Uuid::from_u128(record.id.as_u128()).to_string(),"revision":record.revision.to_string()}),
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
        Error::Invalid => "Invalid application request or inactive/missing owner.",
        Error::Denied => "Administrator authentication or authority denied.",
        Error::Limited { .. } => "Authentication attempt limit reached; wait before retrying.",
        Error::NotFound => "Application not found.",
        Error::Conflict => "The application changed; read its current revision before retrying.",
        Error::Uncertain => {
            "Outcome unknown; inspect the application mutation audit and current state before retrying. Do not repeat creation without reconciliation."
        }
        _ => {
            "Application operation unavailable; check configuration, database, limiter and audit availability."
        }
    }
}
#[cfg(test)]
#[path = "../../tests/unit/operator/applications.rs"]
mod tests;
