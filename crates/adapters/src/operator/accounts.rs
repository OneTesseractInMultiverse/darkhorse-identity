use super::{
    command::Command,
    output::{Failure, Output},
};
use crate::{
    authentication_configuration, deployment_environment::DeploymentEnvironment,
    login_admission::SharedLoginAdmission, password::PasswordPreparation, redis_configuration,
    redis_limiter::RedisLimiter,
};
use darkhorse_application::operator_accounts;
use darkhorse_domain::{
    identity::OperationId,
    operator_accounts::{Error, Operation, Request},
};
mod input;

pub(super) async fn run(command: Command, stdin: bool) -> Result<Output, Failure> {
    let operation = operation(command)?;
    let input = if stdin {
        input::read(std::io::stdin().lock())?
    } else {
        input::interactive(matches!(operation, Operation::Change { .. })).await?
    };
    let request = Request::new(operation, input.reason.as_deref()).map_err(message)?;
    let id = operation_id()?;
    perform(id, request, &input)
        .await
        .map_err(|error| failure(error, id))
}
async fn perform(id: OperationId, request: Request, input: &input::Input) -> Result<Output, Error> {
    let authentication = authentication_configuration::load(DeploymentEnvironment)
        .map_err(|_| Error::Unavailable)?
        .ok_or(Error::Unavailable)?;
    let redis = redis_configuration::load(DeploymentEnvironment).map_err(|_| Error::Unavailable)?;
    let store = super::connect().await.map_err(|_| Error::Unavailable)?;
    let result = execute(&store, authentication, redis, id, request, input).await;
    store.close().await;
    result
}
async fn execute(
    store: &crate::postgres::PostgresStore,
    authentication: authentication_configuration::AuthenticationSettings,
    redis: redis_configuration::RedisSettings,
    id: OperationId,
    request: Request,
    input: &input::Input,
) -> Result<Output, Error> {
    store
        .bind_login_key(authentication.key_digest())
        .await
        .map_err(|_| Error::Unavailable)?;
    let limiter = RedisLimiter::new(store.clone(), redis).map_err(|_| Error::Unavailable)?;
    let admission = SharedLoginAdmission::new(limiter, authentication.key);
    let operation = request.operation();
    let result = operator_accounts::run(
        store,
        &admission,
        &PasswordPreparation::default(),
        id,
        request,
        &input.email,
        &input.password,
    )
    .await?;
    Ok(output(id, operation, result))
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
fn operation_id() -> Result<OperationId, Failure> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| "Cannot generate operation correlation.")?;
    OperationId::from_u128(
        uuid::Builder::from_random_bytes(bytes)
            .into_uuid()
            .as_u128(),
    )
    .map_err(|_| "Cannot generate operation correlation.".into())
}
fn output(id: OperationId, operation: Operation, outcome: operator_accounts::Outcome) -> Output {
    let correlation = uuid::Uuid::from_u128(id.as_u128()).to_string();
    match operation {
        Operation::Show(_) => {
            let mut data = super::project_account(&outcome.account);
            data["operation_id"] = correlation.into();
            Output::record(data)
        }
        Operation::Change { .. } => Output::message(
            format!("Account operation completed. Correlation: {correlation}"),
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
