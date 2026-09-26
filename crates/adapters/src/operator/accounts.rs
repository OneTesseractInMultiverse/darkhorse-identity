use super::authenticated;
use super::localization::Locale;
use super::{
    command::Command,
    output::{Failure, Output},
};
use crate::password::PasswordPreparation;
use darkhorse_application::operator_accounts;
use darkhorse_domain::{
    identity::OperationId,
    operator_accounts::{Error, Operation, Request},
};
mod listing;
enum AccountRequest {
    Account(Request),
    Directory(darkhorse_domain::operator_directory::Request),
}

pub(super) async fn run(command: Command, stdin: bool, locale: Locale) -> Result<Output, Failure> {
    let mutation = matches!(command, Command::Change { .. });
    let input = authenticated::credentials(stdin, mutation, locale).await?;
    let request = request(command, input.reason.as_deref())?;
    let id = super::operation_id()?;
    perform(id, request, &input)
        .await
        .map_err(|error| failure(error, id))
}
async fn perform(
    id: OperationId,
    request: AccountRequest,
    input: &authenticated::Input,
) -> Result<Output, Error> {
    let context = authenticated::connect().await?;
    let result = execute(&context, id, request, input).await;
    context.store.close().await;
    result
}
async fn execute(
    context: &authenticated::Context,
    id: OperationId,
    request: AccountRequest,
    input: &authenticated::Input,
) -> Result<Output, Error> {
    let store = &context.store;
    let admission = &context.admission;
    match request {
        AccountRequest::Account(request) => {
            let operation = request.operation();
            let result = operator_accounts::run(
                store,
                admission,
                &PasswordPreparation::default(),
                id,
                request,
                &input.email,
                &input.password,
            )
            .await?;
            Ok(output(id, operation, result))
        }
        AccountRequest::Directory(request) => {
            let result = operator_accounts::run(
                &store.operator_directory(),
                admission,
                &PasswordPreparation::default(),
                id,
                request,
                &input.email,
                &input.password,
            )
            .await?;
            Ok(listing::output(id, result))
        }
    }
}
fn request(command: Command, reason: Option<&str>) -> Result<AccountRequest, Failure> {
    match command {
        Command::Accounts(query) if reason.is_none() => Ok(AccountRequest::Directory(query)),
        command => Ok(AccountRequest::Account(
            Request::new(operation(command)?, reason).map_err(message)?,
        )),
    }
}

fn operation(command: Command) -> Result<Operation, Failure> {
    match command {
        Command::Account(id) => Ok(Operation::Show(id)),
        Command::Change {
            id,
            revision,
            action,
        } => Ok(Operation::Change {
            target: id,
            revision,
            action,
        }),
        _ => Err(Failure::usage()),
    }
}
fn output(id: OperationId, operation: Operation, outcome: operator_accounts::Outcome) -> Output {
    let correlation = uuid::Uuid::from_u128(id.as_u128()).to_string();
    match operation {
        Operation::Show(_) => {
            let mut data = super::project_account(&outcome.account);
            data["operation_id"] = correlation.into();
            Output::record(data)
        }
        Operation::Change { .. } => Output::localized_message(
            format!("Account operation completed. Correlation: {correlation}"),
            format!("Operación de cuenta completada. Correlación: {correlation}"),
            serde_json::json!({"completed":true,"changed":outcome.changed,"operation_id":correlation,"revision":outcome.account.revision}),
        ),
    }
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
        Error::Invalid => {
            "Invalid account request; mutations require a bounded reason without control characters."
        }
        Error::Denied => "Administrator authentication or authority denied.",
        Error::Limited { .. } => "Authentication attempt limit reached; wait before retrying.",
        Error::NotFound => "Principal not found.",
        Error::Conflict => "The principal changed; read its current revision before retrying.",
        Error::PolicyRejected => "Account operation violates a directory invariant.",
        Error::Unavailable => {
            "Account operation unavailable; check login configuration, database, limiter and audit availability."
        }
        Error::Uncertain => {
            "Outcome unknown; inspect authoritative audit and account state before retrying."
        }
    }
}
