mod accounts;
mod authenticated;
mod cancellation;
mod catalog;
mod catalog_details;
pub mod cli;
pub mod command;
pub mod input;
mod migrations;
pub mod output;
pub mod signing;
mod terminal;

use crate::{database_configuration, password::PasswordPreparation, postgres::PostgresStore};
use command::Command;
use darkhorse_application::{
    bootstrap::{self, BootstrapError, BootstrapRequest},
    directory::{AccountRecord, DirectoryFailure},
};
use darkhorse_domain::AccountStatus;

use output::{Failure, Output};
pub mod confirmation;

pub async fn run(command: Command, auth_stdin: bool) -> Result<Output, Failure> {
    match command {
        Command::CatalogShow(target) if auth_stdin => catalog_details::run(target, true).await,
        Command::CatalogShow(target) => {
            cancellation::run(catalog_details::run(target, false)).await
        }
        Command::Catalog(request) if auth_stdin => catalog::run(request, true).await,
        Command::Catalog(request) => cancellation::run(catalog::run(request, false)).await,
        Command::RedisStatus => redis_status::run().await,
        Command::LimiterFence => limiter::run(limiter::Operation::Fence).await,
        Command::LimiterActivate => limiter::run(limiter::Operation::Activate).await,
        Command::LimiterStatus => limiter::run(limiter::Operation::Status).await,
        Command::LimiterInspect(id) => limiter::run(limiter::Operation::Inspect(id)).await,
        Command::Signing(operation) => signing::run(operation).await,
        Command::Serve => Err("Use the HTTP composition root for serve.".into()),
        Command::Bootstrap { stdin: false } => cancellation::run(run_bootstrap(false)).await,
        Command::Bootstrap { stdin: true } => run_bootstrap(true).await,
        Command::Account(_) | Command::Accounts(_) | Command::Change { .. } if auth_stdin => {
            accounts::run(command, true).await
        }
        Command::Account(_) | Command::Accounts(_) | Command::Change { .. } => {
            cancellation::run(accounts::run(command, false)).await
        }
        command => {
            let store = connect(32).await?;
            let result = run_database_command(&store, command).await;
            store.close().await;
            result
        }
    }
}

async fn connect(max_connections: u32) -> Result<PostgresStore, &'static str> {
    let settings =
        database_configuration::load(crate::deployment_environment::DeploymentEnvironment)
            .map_err(|_| "Invalid database configuration; check DARKHORSE_DATABASE_* settings.")?;
    PostgresStore::connect(connection_limit(settings, max_connections))
        .await
        .map_err(directory_message)
}

async fn run_bootstrap(stdin: bool) -> Result<Output, Failure> {
    let input = if stdin {
        input::read_json(std::io::stdin().lock())?
    } else {
        input::interactive().await?
    };
    let store = connect(32).await?;
    let preparation = PasswordPreparation::default();
    let result = bootstrap::bootstrap(
        &store,
        &preparation,
        BootstrapRequest {
            email: &input.email,
            first_name: &input.first_name,
            last_name: &input.last_name,
            password: &input.password,
        },
    )
    .await;
    store.close().await;
    let principal = result.map_err(bootstrap_message)?;
    let id = uuid::Uuid::from_u128(principal.as_u128());
    Ok(Output::message(
        format!("Administrator initialized: {id}"),
        serde_json::json!({"principal_id":id.to_string()}),
    ))
}

async fn run_database_command(store: &PostgresStore, command: Command) -> Result<Output, Failure> {
    match command {
        Command::Migrate => migrations::apply(store).await,
        Command::MigrationInspect(id) => migrations::inspect(store, id).await,
        _ => Err("Invalid database operation.".into()),
    }
}

fn project_account(record: &AccountRecord) -> serde_json::Value {
    serde_json::json!({ "id":uuid::Uuid::from_u128(record.id.as_u128()).to_string(), "email":record.profile.email(), "first_name":record.profile.first_name(), "last_name":record.profile.last_name(), "active":record.status == AccountStatus::Active, "credential_epoch":record.credential_epoch, "revision":record.revision, "platform_administrator":record.administrator })
}

fn bootstrap_message(error: BootstrapError) -> &'static str {
    match error {
        BootstrapError::AlreadyInitialized => "Administrator bootstrap is already complete.",
        BootstrapError::Invalid(_) => {
            "Invalid profile or password; review the documented input limits."
        }
        BootstrapError::SecretPreparation => "Cannot prepare credentials.",
        BootstrapError::Storage => {
            "Bootstrap could not be completed; verify database state before retrying."
        }
    }
}

fn directory_message(error: DirectoryFailure) -> &'static str {
    match error {
        DirectoryFailure::Unavailable => {
            "Database operation failed; verify configuration and database state."
        }
        DirectoryFailure::NotFound => "Principal not found.",
        DirectoryFailure::Conflict => {
            "The principal changed; read its current revision before retrying."
        }
        DirectoryFailure::Policy(_) => "Account operation violates a directory invariant.",
    }
}

#[cfg(test)]
#[path = "../../tests/unit/operator/mod.rs"]
mod tests;

mod limiter;
mod redis_status;

pub(super) fn operation_id() -> Result<darkhorse_domain::identity::OperationId, Failure> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| "Cannot generate operation correlation.")?;
    darkhorse_domain::identity::OperationId::from_u128(
        uuid::Builder::from_random_bytes(bytes)
            .into_uuid()
            .as_u128(),
    )
    .map_err(|_| "Cannot generate operation correlation.".into())
}

fn connection_limit(
    mut settings: database_configuration::DatabaseSettings,
    limit: u32,
) -> database_configuration::DatabaseSettings {
    settings.max_connections = settings.max_connections.min(limit);
    settings
}
