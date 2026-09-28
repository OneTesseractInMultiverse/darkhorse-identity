use super::{
    authenticated,
    output::{Failure, Output},
};
use crate::password::PasswordPreparation;
use darkhorse_application::operator_accounts;
use darkhorse_domain::{
    identity::OperationId, localization::Locale, operator_accounts::Error,
    operator_client_rotation::Request, operator_client_secrets::Target,
};

pub(super) async fn run(
    target: Target,
    revision: u64,
    overlap_seconds: u16,
    locale: Locale,
) -> Result<Output, Failure> {
    let input = authenticated::credentials(true, true, locale).await?;
    let request = Request::new(
        target,
        revision,
        overlap_seconds,
        input.reason.as_deref().unwrap_or(""),
    )
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
        &context
            .store
            .operator_client_rotation(crate::registration::OsRegistrationEntropy),
        &context.admission,
        &PasswordPreparation::default(),
        id,
        request.clone(),
        &input.email,
        &input.password,
    )
    .await;
    context.store.close().await;
    result.map(|rotated| output(id, &request, rotated))
}

fn output(
    id: OperationId,
    request: &Request,
    rotated: crate::postgres::operator_client_rotation::RotatedSecret,
) -> Output {
    let target = request.target();
    Output::one_time_secret(
        serde_json::json!({
            "completed": true,
            "operation_id": uuid::Uuid::from_u128(id.as_u128()).to_string(),
            "application_id": uuid::Uuid::from_u128(target.application.as_u128()).to_string(),
            "client_id": uuid::Uuid::from_u128(target.client.as_u128()).to_string(),
            "secret_id": uuid::Uuid::from_u128(rotated.secret_id.as_u128()).to_string(),
            "revision": rotated.revision.to_string(),
            "overlap_seconds": request.overlap_seconds()
        }),
        rotated.secret,
    )
}

fn failure(error: Error, id: OperationId) -> Failure {
    let mut data = serde_json::json!({
        "operation_id": uuid::Uuid::from_u128(id.as_u128()).to_string()
    });
    if let Error::Limited { retry_after_ms } = error {
        data["retry_after_ms"] = retry_after_ms.into();
    }
    Failure::from(message(error)).with_data(data)
}

fn message(error: Error) -> &'static str {
    match error {
        Error::Invalid => "Invalid client-secret rotation request.",
        Error::Denied => "Administrator authentication or authority denied.",
        Error::Limited { .. } => "Authentication attempt limit reached; wait before retrying.",
        Error::NotFound => "Client not found in the requested application.",
        Error::Conflict => "The client changed; read its current revision before retrying.",
        Error::Uncertain => {
            "Outcome unknown; inspect the rotation audit and current secret inventory before choosing a fresh rotation. The lost secret cannot be recovered."
        }
        _ => {
            "Client-secret rotation unavailable; check configuration, database, limiter and audit availability."
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/operator/client_secret_rotation.rs"]
mod tests;
