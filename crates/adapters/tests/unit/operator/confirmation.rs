use super::*;
use crate::operator::signing::Operation;
#[test]
fn every_mutation_requires_confirmation_including_stdin_and_legacy_operations() {
    for command in [
        Command::Migrate,
        Command::Signing(Operation::Status),
        Command::Bootstrap { stdin: true },
        Command::Bootstrap { stdin: false },
        Command::LimiterFence,
        Command::LimiterActivate,
        Command::Signing(Operation::Generate(0)),
        Command::Signing(Operation::Import(0)),
        Command::Signing(Operation::Activate {
            kid: [0; 32],
            revision: 0,
        }),
        Command::Signing(Operation::Retire {
            kid: [0; 32],
            revision: 0,
        }),
        Command::Change {
            id: darkhorse_domain::identity::PrincipalId::from_u128(1).unwrap(),
            revision: 0,
            action: darkhorse_domain::directory::AccountAction::RevokeAll,
        },
    ] {
        assert!(requires_confirmation(&command));
    }
    for command in [
        Command::Serve,
        Command::RedisStatus,
        Command::LimiterStatus,
        Command::Account(darkhorse_domain::identity::PrincipalId::from_u128(1).unwrap()),
    ] {
        assert!(!requires_confirmation(&command));
    }
    assert!(consumes_stdin(&Command::Bootstrap { stdin: true }));
    assert!(consumes_stdin(&Command::Signing(Operation::Import(0))));
    assert!(!consumes_stdin(&Command::Migrate));
}
#[test]
fn explicit_confirmation_rejects_eof_partial_words_and_forged_lines() {
    assert!(accept("yes\n").is_ok());
    assert!(accept("yes\r\n").is_ok());
    for line in ["", "yes", "no\n", "yes!\n", "yes\nforged", "\x1b[0myes\n"] {
        assert_eq!(accept(line).unwrap_err().exit_code(), 3);
    }
}
#[test]
fn automation_never_prompts_or_consumes_protected_input_for_confirmation() {
    assert_eq!(
        decision(&Command::Migrate, false, true, Format::Human),
        Decision::Prompt
    );
    assert_eq!(
        decision(&Command::Migrate, false, false, Format::Human),
        Decision::Deny
    );
    assert_eq!(
        decision(&Command::Migrate, false, true, Format::Json),
        Decision::Deny
    );
    assert_eq!(
        decision(
            &Command::Bootstrap { stdin: true },
            false,
            true,
            Format::Human
        ),
        Decision::Deny
    );
    assert_eq!(
        decision(&Command::Migrate, true, false, Format::Json),
        Decision::Proceed
    );
    assert_eq!(
        decision(
            &Command::Account(darkhorse_domain::identity::PrincipalId::from_u128(1).unwrap()),
            false,
            false,
            Format::Json
        ),
        Decision::Proceed
    );
}
