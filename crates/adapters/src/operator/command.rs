use darkhorse_domain::{
    directory::AccountAction,
    identity::{OperationId, PrincipalId},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Serve,
    Migrate,
    MigrationInspect(OperationId),
    RedisStatus,
    LimiterFence,
    LimiterActivate,
    LimiterStatus,
    LimiterInspect(OperationId),
    Signing(super::signing::Operation),
    Bootstrap {
        stdin: bool,
    },
    Account(PrincipalId),
    Accounts(darkhorse_domain::operator_directory::Request),
    Catalog(darkhorse_domain::operator_catalog::Request),
    CatalogShow(darkhorse_application::registration::ReadTarget),
    ApplicationMutation(darkhorse_domain::operator_applications::Operation),
    Change {
        id: PrincipalId,
        revision: u64,
        action: AccountAction,
    },
}

pub(super) fn identifier(value: &str) -> Result<PrincipalId, &'static str> {
    let value = uuid::Uuid::parse_str(value).map_err(|_| "Invalid principal identifier.")?;
    PrincipalId::from_u128(value.as_u128()).map_err(|_| "Invalid principal identifier.")
}

pub(super) fn counter(value: &str) -> Result<u64, &'static str> {
    value
        .parse::<u64>()
        .ok()
        .filter(|&v| v <= i64::MAX as u64)
        .ok_or("Invalid expected revision.")
}

pub fn requires_confirmation(command: &Command) -> bool {
    !matches!(
        command,
        Command::Serve
            | Command::MigrationInspect(_)
            | Command::Account(_)
            | Command::Accounts(_)
            | Command::Catalog(_)
            | Command::CatalogShow(_)
            | Command::RedisStatus
            | Command::LimiterStatus
            | Command::LimiterInspect(_)
            | Command::Signing(super::signing::Operation::Inspect(_))
    )
}

#[cfg(test)]
#[path = "../../tests/unit/operator/command.rs"]
mod tests;

pub(super) fn operation_identifier(value: &str) -> Result<OperationId, &'static str> {
    let value = uuid::Uuid::parse_str(value).map_err(|_| "Invalid operation identifier.")?;
    OperationId::from_u128(value.as_u128()).map_err(|_| "Invalid operation identifier.")
}

pub(super) fn application_identifier(
    value: &str,
) -> Result<darkhorse_domain::identity::ApplicationId, &'static str> {
    let value = uuid::Uuid::parse_str(value).map_err(|_| "Invalid application identifier.")?;
    darkhorse_domain::identity::ApplicationId::from_u128(value.as_u128())
        .map_err(|_| "Invalid application identifier.")
}

pub(super) fn client_identifier(
    value: &str,
) -> Result<darkhorse_domain::identity::ClientId, &'static str> {
    let value = uuid::Uuid::parse_str(value).map_err(|_| "Invalid client identifier.")?;
    darkhorse_domain::identity::ClientId::from_u128(value.as_u128())
        .map_err(|_| "Invalid client identifier.")
}
