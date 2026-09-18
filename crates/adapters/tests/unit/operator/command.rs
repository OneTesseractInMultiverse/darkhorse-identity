use super::*;

fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

#[test]
fn accepts_only_explicit_operations_and_never_secret_arguments() {
    assert_eq!(parse(&[]), Ok(Command::Serve));
    assert_eq!(parse(&args(&["serve"])), Ok(Command::Serve));
    assert_eq!(parse(&args(&["--help"])), Ok(Command::Help));
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
