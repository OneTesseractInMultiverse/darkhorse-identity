use darkhorse_domain::{
    directory::AccountAction,
    identity::{OperationId, PrincipalId},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Serve,
    Migrate,
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

pub fn requires_confirmation(command: Command) -> bool {
    !matches!(
        command,
        Command::Serve
            | Command::Account(_)
            | Command::RedisStatus
            | Command::LimiterStatus
            | Command::LimiterInspect(_)
    )
}

#[cfg(test)]
#[path = "../../tests/unit/operator/command.rs"]
mod tests;

pub(super) fn operation_identifier(value: &str) -> Result<OperationId, &'static str> {
    let value = uuid::Uuid::parse_str(value).map_err(|_| "Invalid operation identifier.")?;
    OperationId::from_u128(value.as_u128()).map_err(|_| "Invalid operation identifier.")
}
