use super::*;
use darkhorse_domain::{
    directory::{DirectoryError, Profile},
    identity::PrincipalId,
};

#[test]
fn projection_and_errors_expose_only_intended_operator_data() {
    let record = AccountRecord {
        id: PrincipalId::from_u128(1).unwrap(),
        profile: Profile::new("a@b.com", "A", "B").unwrap(),
        status: AccountStatus::Active,
        credential_epoch: 2,
        revision: 3,
        administrator: true,
        eligible_administrator: true,
    };
    let value = project_account(&record);
    assert_eq!(value["email"], "a@b.com");
    assert_eq!(value["revision"], 3);
    assert!(value.get("password").is_none());
    assert!(value.get("verifier").is_none());
    for error in [
        BootstrapError::AlreadyInitialized,
        BootstrapError::Invalid(DirectoryError::Email),
        BootstrapError::SecretPreparation,
        BootstrapError::Storage,
    ] {
        assert!(!bootstrap_message(error).is_empty());
    }
    for error in [
        DirectoryFailure::Unavailable,
        DirectoryFailure::NotFound,
        DirectoryFailure::Conflict,
        DirectoryFailure::Policy(DirectoryError::LastAdministrator),
    ] {
        assert!(!directory_message(error).is_empty());
    }
}

#[test]
fn account_pool_respects_smaller_configuration_and_caps_larger_pools() {
    for (configured, expected) in [("1", 1), ("2", 2), ("32", 2)] {
        let settings = database_configuration::load(envbind::MapEnvironment::from_pairs([
            (
                "DARKHORSE_DATABASE_URL",
                "postgres://unit:fixture@localhost/directory",
            ),
            ("DARKHORSE_DATABASE_POOL_SIZE", configured),
        ]))
        .unwrap();
        assert_eq!(connection_limit(settings, 2).max_connections, expected);
    }
}
