use super::*;
use crate::operator::{command::Command, output::Format};
use std::ffi::OsString;

fn parse(parts: &[&str]) -> Result<Plan, crate::operator::output::Failure> {
    invocation(&parts.iter().map(OsString::from).collect::<Vec<_>>())
}
fn run(parts: &[&str]) -> Invocation {
    match parse(parts).unwrap() {
        Plan::Run(value) => value,
        _ => panic!("expected command"),
    }
}
#[test]
fn hierarchy_and_legacy_aliases_resolve_to_identical_operations() {
    for (canonical, legacy) in [
        (vec!["operator", "migrate"], vec!["migrate"]),
        (
            vec!["operator", "bootstrap", "--stdin"],
            vec!["bootstrap", "--stdin"],
        ),
        (
            vec!["operator", "signing", "status"],
            vec!["signing-status"],
        ),
        (vec!["operator", "limiter", "fence"], vec!["limiter-fence"]),
        (vec!["operator", "redis", "status"], vec!["redis-status"]),
        (
            vec![
                "operator",
                "account",
                "show",
                "00000000-0000-0000-0000-000000000001",
            ],
            vec!["account", "00000000-0000-0000-0000-000000000001"],
        ),
    ] {
        assert_eq!(run(&canonical).command, run(&legacy).command);
    }
    assert_eq!(run(&[]).command, Command::Serve);
    assert_eq!(run(&["serve"]).command, Command::Serve);
    assert_eq!(
        run(&["--output", "json", "operator", "migrate", "--yes"]).format,
        Format::Json
    );
    assert!(run(&["migrate", "--yes"]).confirmed);
}
#[test]
fn help_and_version_are_bounded_static_information() {
    for parts in [
        vec!["--help"],
        vec!["help"],
        vec!["operator", "--help"],
        vec!["operator", "signing", "--help"],
        vec!["--version"],
    ] {
        let Plan::Display(text) = parse(&parts).unwrap() else {
            panic!("expected help")
        };
        assert!(text.len() < 16_384);
        assert!(!text.contains('\x1b'));
    }
}
#[test]
fn errors_never_echo_supplied_values_and_arguments_are_bounded() {
    let secret = "marker-secret-\x1b[31m\nforged";
    for args in [
        vec!["--output", "json", "bootstrap", "--yes"],
        vec!["serve", "--auth-stdin"],
        vec!["migrate", "--auth-stdin"],
        vec![
            "--output",
            "json",
            "account",
            "00000000-0000-0000-0000-000000000001",
        ],
        vec!["unknown", secret],
        vec!["bootstrap", "--password", secret],
        vec!["account", secret],
        vec![
            "deactivate",
            "00000000-0000-0000-0000-000000000001",
            "9223372036854775808",
        ],
        vec!["operator", "account"],
        vec!["serve", "--output", "json"],
        vec!["migrate", "--yes", "--yes"],
        vec!["operator", "signing", "import", "0"],
        vec!["account", "$(touch marker)"],
    ] {
        let error = parse(&args).unwrap_err();
        assert_eq!(error.exit_code(), 2);
        assert!(!format!("{error:?}").contains(secret));
    }
    for args in [
        vec!["x".repeat(1025)],
        vec!["x".into(); 33],
        vec!["x".repeat(1024); 5],
    ] {
        assert_eq!(
            invocation(&args.into_iter().map(OsString::from).collect::<Vec<_>>())
                .unwrap_err()
                .exit_code(),
            2
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        assert_eq!(
            invocation(&[OsString::from_vec(vec![255])])
                .unwrap_err()
                .exit_code(),
            2
        );
    }
}

#[test]
fn account_authentication_input_is_explicit_and_not_a_bypass_flag() {
    let command = run(&[
        "--output",
        "json",
        "operator",
        "account",
        "show",
        "00000000-0000-0000-0000-000000000001",
        "--auth-stdin",
    ]);
    assert!(command.auth_stdin);
    assert!(!run(&["account", "00000000-0000-0000-0000-000000000001"]).auth_stdin);
}

#[test]
fn migration_inspection_is_read_only_and_uses_a_typed_operation_id() {
    let invocation = run(&[
        "operator",
        "migrate",
        "inspect",
        "00000000-0000-0000-0000-000000000001",
    ]);
    assert!(!crate::operator::command::requires_confirmation(
        invocation.command
    ));
    for value in ["invalid", "00000000-0000-0000-0000-000000000000"] {
        assert!(parse(&["operator", "migrate", "inspect", value]).is_err());
    }
    assert!(parse(&["operator", "migrate", "inspect"]).is_err());
}
