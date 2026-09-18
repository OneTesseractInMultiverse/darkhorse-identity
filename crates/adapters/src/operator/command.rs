use darkhorse_domain::{AccountStatus, directory::AccountAction, identity::PrincipalId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Serve,
    Help,
    Migrate,
    RedisStatus,
    LimiterFence,
    LimiterActivate,
    LimiterStatus,
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

pub fn parse(args: &[String]) -> Result<Command, &'static str> {
    let args: Vec<_> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        [] | ["serve"] => Ok(Command::Serve),
        ["--help"] | ["help"] => Ok(Command::Help),
        ["migrate"] => Ok(Command::Migrate),
        ["redis-status"] => Ok(Command::RedisStatus),
        ["limiter-fence"] => Ok(Command::LimiterFence),
        ["limiter-activate"] => Ok(Command::LimiterActivate),
        ["limiter-status"] => Ok(Command::LimiterStatus),
        ["signing-status"] => Ok(Command::Signing(super::signing::Operation::Status)),
        ["signing-generate", revision] => Ok(Command::Signing(
            super::signing::Operation::Generate(counter(revision)?),
        )),
        ["signing-import", "--stdin", revision] => Ok(Command::Signing(
            super::signing::Operation::Import(counter(revision)?),
        )),
        ["signing-activate", kid, revision] => {
            Ok(Command::Signing(super::signing::Operation::Activate {
                kid: super::signing::identifier(kid)?,
                revision: counter(revision)?,
            }))
        }
        ["signing-retire", kid, revision] => {
            Ok(Command::Signing(super::signing::Operation::Retire {
                kid: super::signing::identifier(kid)?,
                revision: counter(revision)?,
            }))
        }
        ["bootstrap"] => Ok(Command::Bootstrap { stdin: false }),
        ["bootstrap", "--stdin"] => Ok(Command::Bootstrap { stdin: true }),
        ["account", id] => Ok(Command::Account(identifier(id)?)),
        [
            operation @ ("deactivate" | "reactivate" | "revoke-all"),
            id,
            revision,
        ] => Ok(Command::Change {
            id: identifier(id)?,
            revision: counter(revision)?,
            action: action(operation),
        }),
        _ => Err("Invalid command; use --help."),
    }
}

fn identifier(value: &str) -> Result<PrincipalId, &'static str> {
    let value = uuid::Uuid::parse_str(value).map_err(|_| "Invalid principal identifier.")?;
    PrincipalId::from_u128(value.as_u128()).map_err(|_| "Invalid principal identifier.")
}

fn counter(value: &str) -> Result<u64, &'static str> {
    value
        .parse::<u64>()
        .ok()
        .filter(|&v| v <= i64::MAX as u64)
        .ok_or("Invalid expected revision.")
}

fn action(operation: &str) -> AccountAction {
    match operation {
        "deactivate" => AccountAction::SetStatus(AccountStatus::Inactive),
        "reactivate" => AccountAction::SetStatus(AccountStatus::Active),
        _ => AccountAction::RevokeAll,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/operator/command.rs"]
mod tests;
