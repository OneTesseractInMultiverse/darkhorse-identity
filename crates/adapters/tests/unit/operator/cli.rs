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
        &invocation.command
    ));
    for value in ["invalid", "00000000-0000-0000-0000-000000000000"] {
        assert!(parse(&["operator", "migrate", "inspect", value]).is_err());
    }
    assert!(parse(&["operator", "migrate", "inspect"]).is_err());
}

#[test]
fn account_listing_is_authenticated_bounded_and_read_only() {
    use crate::operator::command::requires_confirmation;
    let parsed = run(&["operator", "account", "list"]);
    assert!(!requires_confirmation(&parsed.command));
    let Command::Accounts(query) = parsed.command else {
        panic!("expected directory query")
    };
    assert_eq!(query.query().limit, 25);
    assert_eq!(query.query().search, "");
    let parsed = run(&[
        "--auth-stdin",
        "--output",
        "json",
        "operator",
        "account",
        "list",
        "--search",
        "Ada",
        "--status",
        "inactive",
        "--limit",
        "1",
        "--after",
        "00000000-0000-0000-0000-000000000001",
    ]);
    let Command::Accounts(query) = parsed.command else {
        panic!("expected directory query")
    };
    assert_eq!(query.query().search, "Ada");
    assert_eq!(
        query.query().status,
        Some(darkhorse_domain::AccountStatus::Inactive)
    );
    assert_eq!(query.query().after.unwrap().as_u128(), 1);
    for args in [
        vec!["--limit", "0"],
        vec!["--limit", "26"],
        vec!["--status", "unknown"],
        vec!["--after", "00000000-0000-0000-0000-000000000000"],
        vec!["--search", "trim me "],
        vec!["--limit", "2", "--limit", "3"],
        vec!["--output", "json"],
    ] {
        let mut command = vec!["operator", "account", "list"];
        command.extend(args);
        assert!(parse(&command).is_err());
    }
    assert!(parse(&["operator", "account", "list", "--search", &"x".repeat(101)]).is_err());
}

#[test]
fn catalog_listing_requires_fresh_authentication_and_typed_bounded_selectors() {
    use darkhorse_domain::operator_catalog::Target;
    for (parts, target) in [
        (
            vec!["operator", "application", "list"],
            Target::Applications,
        ),
        (
            vec![
                "operator",
                "client",
                "list",
                "00000000-0000-0000-0000-000000000010",
            ],
            Target::Clients(darkhorse_domain::identity::ApplicationId::from_u128(16).unwrap()),
        ),
    ] {
        let parsed = run(&parts);
        assert!(!crate::operator::command::requires_confirmation(
            &parsed.command
        ));
        let Command::Catalog(request) = parsed.command else {
            panic!("catalog request")
        };
        assert_eq!(request.target(), target);
        assert_eq!(request.query().limit, 25);
    }
    for parts in [
        vec!["operator", "application", "list", "--limit", "26"],
        vec!["operator", "application", "list", "--output", "json"],
        vec!["operator", "client", "list"],
        vec![
            "operator",
            "client",
            "list",
            "00000000-0000-0000-0000-000000000000",
        ],
        vec!["operator", "application", "list", "--after", "invalid"],
        vec![
            "operator",
            "application",
            "list",
            "--after",
            "00000000-0000-0000-0000-000000000000",
        ],
        vec!["operator", "application", "create"],
        vec!["operator", "client", "rotate-secret"],
    ] {
        assert!(parse(&parts).is_err());
    }
    let Command::Catalog(request) = run(&[
        "--auth-stdin",
        "--output",
        "json",
        "operator",
        "application",
        "list",
        "--search=--literal",
        "--status",
        "inactive",
        "--after",
        "00000000-0000-0000-0000-000000000020",
        "--limit",
        "1",
    ])
    .command
    else {
        panic!("catalog request")
    };
    assert_eq!(request.query().search, "--literal");
    assert_eq!(request.query().active, Some(false));
    assert_eq!(request.query().after.unwrap().get(), 32);
}

#[test]
fn catalog_details_require_typed_targets_and_protected_json_without_list_selectors() {
    use darkhorse_application::registration::ReadTarget;
    let app = "00000000-0000-0000-0000-000000000010";
    let client = "00000000-0000-0000-0000-000000000020";
    for parts in [
        vec!["operator", "application", "show", app],
        vec!["operator", "client", "show", app, client],
    ] {
        let parsed = run(&parts);
        assert!(!crate::operator::command::requires_confirmation(
            &parsed.command
        ));
        let Command::CatalogShow(target) = parsed.command else {
            panic!("detail target")
        };
        match target {
            ReadTarget::Application(id) => assert_eq!(id.as_u128(), 16),
            ReadTarget::Client {
                application,
                client,
            } => {
                assert_eq!(application.as_u128(), 16);
                assert_eq!(client.as_u128(), 32);
            }
        }
        let mut json = vec!["--auth-stdin", "--output", "json"];
        json.extend(parts.clone());
        assert!(parse(&json).is_ok());
        for invalid in [
            vec!["--limit", "1"],
            vec!["--search", "name"],
            vec!["--after", client],
            vec!["--status", "active"],
            vec!["--output", "json"],
        ] {
            let mut args = parts.clone();
            args.extend(invalid);
            assert!(parse(&args).is_err());
        }
    }
    for parts in [
        vec!["operator", "application", "show"],
        vec!["operator", "client", "show", app],
        vec!["operator", "client", "show", app, "invalid"],
        vec![
            "operator",
            "client",
            "show",
            app,
            "00000000-0000-0000-0000-000000000000",
        ],
        vec![
            "operator",
            "application",
            "show",
            "00000000-0000-0000-0000-000000000000",
        ],
    ] {
        assert!(parse(&parts).is_err());
    }
}

#[test]
fn application_writes_require_complete_explicit_specs_and_scoped_revisions() {
    let owner = "00000000-0000-0000-0000-000000000001";
    let app = "00000000-0000-0000-0000-000000000010";
    for operation in [vec!["create"], vec!["update", app, "7"]] {
        let mut args = vec![
            "--auth-stdin",
            "--output",
            "json",
            "--yes",
            "operator",
            "application",
        ];
        args.extend(operation);
        args.extend(["--name", "Portal", "--owner", owner, "--status", "active"]);
        let invocation = parse(&args);
        assert!(
            invocation.is_ok(),
            "complete application write must be accepted"
        );
        let command = run(&args);
        assert!(command.confirmed && command.auth_stdin);
        assert!(crate::operator::command::requires_confirmation(
            &command.command
        ));
        for bad in ["", "\nprivate-value", "--not-a-command"] {
            let mut invalid = args.clone();
            let index = invalid.iter().position(|value| *value == owner).unwrap();
            invalid[index] = bad;
            assert!(parse(&invalid).is_err());
        }
        for flag in ["--name", "--owner", "--status"] {
            let mut missing = args.clone();
            let index = missing.iter().position(|value| *value == flag).unwrap();
            missing.drain(index..index + 2);
            assert!(parse(&missing).is_err());
        }
    }
    for args in [
        vec!["operator", "application", "create", app],
        vec!["operator", "application", "update", app],
        vec![
            "operator",
            "application",
            "update",
            app,
            "9223372036854775808",
        ],
        vec!["operator", "client", "create"],
    ] {
        assert!(parse(&args).is_err());
    }
}
#[test]
fn client_updates_require_scoped_revision_and_protected_configuration_input() {
    let args = [
        "--auth-stdin",
        "--output",
        "json",
        "operator",
        "client",
        "update",
        "00000000-0000-0000-0000-000000000001",
        "00000000-0000-0000-0000-000000000002",
        "0",
    ];
    assert!(
        parse(&args).is_ok(),
        "scoped client update must be supported"
    );
    let invocation = run(&args);
    assert!(crate::operator::command::requires_confirmation(
        &invocation.command
    ));
    for args in [
        vec![
            "operator",
            "client",
            "update",
            "00000000-0000-0000-0000-000000000001",
            "00000000-0000-0000-0000-000000000002",
            "0",
        ],
        vec![
            "--auth-stdin",
            "operator",
            "client",
            "update",
            "00000000-0000-0000-0000-000000000001",
            "00000000-0000-0000-0000-000000000002",
        ],
        vec![
            "--auth-stdin",
            "operator",
            "client",
            "update",
            "00000000-0000-0000-0000-000000000001",
            "00000000-0000-0000-0000-000000000002",
            "9223372036854775808",
        ],
    ] {
        assert!(parse(&args).is_err());
    }
}

#[test]
fn client_secret_inventory_and_retirement_are_scoped_and_have_distinct_confirmation_contracts() {
    let app = "00000000-0000-0000-0000-000000000001";
    let client = "00000000-0000-0000-0000-000000000002";
    let secret = "00000000-0000-0000-0000-000000000003";
    let inventory = run(&[
        "--auth-stdin",
        "--output",
        "json",
        "operator",
        "client",
        "secret",
        "list",
        app,
        client,
        "--after",
        secret,
        "--limit",
        "2",
    ]);
    assert!(!crate::operator::command::requires_confirmation(
        &inventory.command
    ));
    let retirement = run(&[
        "--auth-stdin",
        "--output",
        "json",
        "--yes",
        "operator",
        "client",
        "secret",
        "retire",
        app,
        client,
        secret,
        "7",
    ]);
    assert!(crate::operator::command::requires_confirmation(
        &retirement.command
    ));
    for tail in [
        vec!["list", app, client, "--limit", "26"],
        vec!["list", app, client, "--after", "0"],
        vec!["retire", app, client, secret],
        vec!["retire", app, client, secret, "9223372036854775808"],
        vec![
            "retire",
            app,
            client,
            "00000000-0000-0000-0000-000000000000",
            "0",
        ],
    ] {
        let mut parts = vec!["operator", "client", "secret"];
        parts.extend(tail);
        assert!(parse(&parts).is_err());
    }
    assert!(
        parse(&[
            "--output", "json", "operator", "client", "secret", "list", app, client
        ])
        .is_err()
    );
}

#[test]
fn access_lists_require_explicit_scope_and_reject_irrelevant_selectors() {
    let app = "00000000-0000-0000-0000-000000000001";
    for kind in ["resource", "scope", "role", "capability"] {
        let scope = if ["resource", "scope"].contains(&kind) {
            vec![app]
        } else {
            vec!["--application", app]
        };
        let mut args = vec!["--auth-stdin", "--output", "json", "operator", kind, "list"];
        args.extend(scope);
        assert!(matches!(run(&args).command, Command::Catalog(_)));
        args.extend(["--limit", "26"]);
        assert!(parse(&args).is_err());
        assert!(parse(&["operator", kind, "list"]).is_err());
        if kind != "capability" {
            let mut args = vec!["operator", kind, "list"];
            args.extend(if kind == "role" {
                vec!["--all-definitions"]
            } else {
                vec![app]
            });
            args.extend(["--status", "active"]);
            assert!(parse(&args).is_err());
        }
    }
    for kind in ["role", "capability"] {
        assert!(parse(&["operator", kind, "list", "--all-definitions"]).is_ok());
        assert!(
            parse(&[
                "operator",
                kind,
                "list",
                "--all-definitions",
                "--application",
                app
            ])
            .is_err()
        );
    }
}
