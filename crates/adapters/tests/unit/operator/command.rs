use super::*;
use darkhorse_domain::AccountStatus;
fn parse(args: &[String]) -> Result<Command, &'static str> {
    match crate::operator::cli::invocation(
        &args
            .iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>(),
    ) {
        Ok(crate::operator::cli::Plan::Run(value)) => Ok(value.command),
        _ => Err("Invalid command."),
    }
}

fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

#[test]
fn accepts_only_explicit_operations_and_never_secret_arguments() {
    assert_eq!(parse(&[]), Ok(Command::Serve));
    assert_eq!(parse(&args(&["serve"])), Ok(Command::Serve));
    assert_eq!(parse(&args(&["migrate"])), Ok(Command::Migrate));
    assert_eq!(parse(&args(&["redis-status"])), Ok(Command::RedisStatus));
    assert_eq!(parse(&args(&["limiter-fence"])), Ok(Command::LimiterFence));
    assert_eq!(
        parse(&args(&["limiter-activate"])),
        Ok(Command::LimiterActivate)
    );
    assert_eq!(
        parse(&args(&["limiter-status"])),
        Ok(Command::LimiterStatus)
    );
    assert_eq!(
        parse(&args(&["bootstrap"])),
        Ok(Command::Bootstrap { stdin: false })
    );
    assert_eq!(
        parse(&args(&["bootstrap", "--stdin"])),
        Ok(Command::Bootstrap { stdin: true })
    );
    let id = "00000000-0000-0000-0000-000000000001";
    assert_eq!(
        parse(&args(&["account", id])),
        Ok(Command::Account(PrincipalId::from_u128(1).unwrap()))
    );
    for (name, action) in [
        (
            "deactivate",
            AccountAction::SetStatus(AccountStatus::Inactive),
        ),
        (
            "reactivate",
            AccountAction::SetStatus(AccountStatus::Active),
        ),
        ("revoke-all", AccountAction::RevokeAll),
    ] {
        assert_eq!(
            parse(&args(&[name, id, "7"])),
            Ok(Command::Change {
                id: PrincipalId::from_u128(1).unwrap(),
                revision: 7,
                action
            })
        );
    }
    for parts in [
        vec!["unknown"],
        vec!["limiter-activate", "--skip-wait"],
        vec!["bootstrap", "password-secret"],
        vec!["bootstrap", "--stdin", "extra"],
        vec!["account", "invalid"],
        vec!["account", "00000000-0000-0000-0000-000000000000"],
        vec!["deactivate", id, "-1"],
        vec!["deactivate", id, "9223372036854775808"],
        vec!["deactivate", id, "secret"],
    ] {
        let error = parse(&args(&parts)).unwrap_err();
        assert!(!error.contains("password-secret"));
    }
}

#[test]
fn signing_operations_accept_only_bounded_public_arguments() {
    use crate::operator::signing::Operation;
    assert_eq!(
        parse(&args(&["signing-status"])),
        Ok(Command::Signing(Operation::Status))
    );
    assert_eq!(
        parse(&args(&["signing-generate", "0"])),
        Ok(Command::Signing(Operation::Generate(0)))
    );
    assert_eq!(
        parse(&args(&["signing-import", "--stdin", "1"])),
        Ok(Command::Signing(Operation::Import(1)))
    );
    let kid = "A".repeat(43);
    assert_eq!(
        parse(&args(&["signing-activate", &kid, "2"])),
        Ok(Command::Signing(Operation::Activate {
            kid: [0; 32],
            revision: 2
        }))
    );
    assert_eq!(
        parse(&args(&["signing-retire", &kid, "3"])),
        Ok(Command::Signing(Operation::Retire {
            kid: [0; 32],
            revision: 3
        }))
    );
    for bad in [
        vec!["signing-import", "private-key", "0"],
        vec!["signing-activate", "invalid", "0"],
        vec!["signing-generate", "-1"],
        vec!["signing-retire", &kid, "9223372036854775808"],
    ] {
        assert!(parse(&args(&bad)).is_err());
    }
}
#[test]
fn limiter_inspection_is_a_read_with_a_typed_operation_identifier() {
    let operation = darkhorse_domain::identity::OperationId::from_u128(1).unwrap();
    let id = "00000000-0000-0000-0000-000000000001";
    assert_eq!(
        parse(&args(&["operator", "limiter", "inspect", id])),
        Ok(Command::LimiterInspect(operation))
    );
    assert!(!requires_confirmation(&Command::LimiterInspect(operation)));
    for id in ["bad", "00000000-0000-0000-0000-000000000000", "--force"] {
        assert!(parse(&args(&["operator", "limiter", "inspect", id])).is_err());
    }
}
#[test]
fn signing_inspection_is_read_only_and_accepts_only_an_operation_id() {
    use crate::operator::signing::Operation;
    let id = "00000000-0000-0000-0000-000000000001";
    let command = Command::Signing(Operation::Inspect(OperationId::from_u128(1).unwrap()));
    assert_eq!(
        parse(&args(&["operator", "signing", "inspect", id])),
        Ok(command.clone())
    );
    assert!(!requires_confirmation(&command));
    for bad in ["invalid", "00000000-0000-0000-0000-000000000000", "--force"] {
        assert!(parse(&args(&["operator", "signing", "inspect", bad])).is_err());
    }
    assert!(parse(&args(&["operator", "signing", "inspect", id, "extra"])).is_err());
}
#[test]
fn signing_key_identifiers_accept_a_leading_base64url_hyphen() {
    use crate::operator::signing::Operation;
    let kid = format!("-{}", "A".repeat(42));
    let mut bytes = [0; 32];
    bytes[0] = 248;
    for (name, operation) in [
        (
            "activate",
            Operation::Activate {
                kid: bytes,
                revision: 1,
            },
        ),
        (
            "retire",
            Operation::Retire {
                kid: bytes,
                revision: 1,
            },
        ),
    ] {
        assert_eq!(
            parse(&args(&["operator", "signing", name, &kid, "1"])),
            Ok(Command::Signing(operation))
        );
        assert_eq!(
            parse(&args(&[&format!("signing-{name}"), &kid, "1"])),
            Ok(Command::Signing(operation))
        );
    }
}
