//! Fresh-password CLI writes to the shared access catalog.
use super::{
    authenticated,
    output::{Failure, Output},
};
use crate::{password::PasswordPreparation, registration::OsRegistrationEntropy};
use darkhorse_application::{admin_catalog::Written, operator_accounts};
use darkhorse_domain::{
    identity::OperationId, operator_accounts::Error, operator_catalog::MutationRequest,
};

mod input;

pub(super) async fn run() -> Result<Output, Failure> {
    let (request, authentication) = input::read(std::io::stdin().lock())?;
    let operation = super::operation_id()?;
    perform(operation, request, &authentication)
        .await
        .map_err(|error| failure(error, operation))
}

async fn perform(
    operation: OperationId,
    request: MutationRequest,
    authentication: &authenticated::Input,
) -> Result<Output, Error> {
    let context = authenticated::connect().await?;
    let expected_revision = request.policy_revision();
    let result = operator_accounts::run(
        &context.store.operator_access_catalog(OsRegistrationEntropy),
        &context.admission,
        &PasswordPreparation::default(),
        operation,
        request,
        &authentication.email,
        &authentication.password,
    )
    .await;
    context.store.close().await;
    let written = result?;
    Ok(output(operation, expected_revision, written))
}

fn output(operation: OperationId, expected_revision: u64, written: Written) -> Output {
    let id = uuid::Uuid::from_u128;
    Output::record(serde_json::json!({
        "completed":true,
        "changed":written.policy_revision > expected_revision,
        "operation_id":id(operation.as_u128()).to_string(),
        "target":target(written.target),
        "policy_revision":written.policy_revision.to_string()
    }))
}

fn target(target: darkhorse_application::admin_catalog::Target) -> serde_json::Value {
    use darkhorse_application::admin_catalog::Target;

    match target {
        Target::Capability(capability) => serde_json::json!({
            "kind":"capability",
            "id":uuid::Uuid::from_u128(capability.as_u128()).to_string()
        }),
        Target::Role(role) => serde_json::json!({
            "kind":"role",
            "id":uuid::Uuid::from_u128(role.as_u128()).to_string()
        }),
        Target::Resource(application, resource) => serde_json::json!({
            "kind":"resource",
            "application_id":uuid::Uuid::from_u128(application.as_u128()).to_string(),
            "id":uuid::Uuid::from_u128(resource.as_u128()).to_string()
        }),
        Target::Scope(application, resource, scope) => serde_json::json!({
            "kind":"scope",
            "application_id":uuid::Uuid::from_u128(application.as_u128()).to_string(),
            "resource_id":uuid::Uuid::from_u128(resource.as_u128()).to_string(),
            "id":uuid::Uuid::from_u128(scope.as_u128()).to_string()
        }),
    }
}

fn failure(error: Error, operation: OperationId) -> Failure {
    let id = uuid::Uuid::from_u128(operation.as_u128()).to_string();
    let mut data = serde_json::json!({"operation_id":id});
    if let Error::Limited { retry_after_ms } = error {
        data["retry_after_ms"] = retry_after_ms.into();
    }
    Failure::from(message(error)).with_data(data)
}

fn message(error: Error) -> &'static str {
    match error {
        Error::Invalid => "Invalid access-catalog mutation or bounded reason.",
        Error::Denied => "Administrator authentication or authority denied.",
        Error::Limited { .. } => "Authentication attempt limit reached; wait before retrying.",
        Error::NotFound => "An access-catalog target or related definition was not found.",
        Error::Conflict => "The policy changed; read its current revision before retrying.",
        Error::PolicyRejected => "The access-catalog change violates a policy invariant.",
        Error::Uncertain => {
            "Outcome unknown; inspect the access-catalog audit and current policy before retrying."
        }
        Error::Unavailable => {
            "Access-catalog mutation unavailable; check configuration, database, limiter and audit availability."
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/operator/access_catalog_mutations.rs"]
mod tests;
