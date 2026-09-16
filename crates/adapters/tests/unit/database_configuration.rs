use super::*;
use envbind::MapEnvironment;

#[test]
fn requires_explicit_connection_and_bounds_configuration_without_a_database() {
    assert!(load(MapEnvironment::new()).is_err());
    for url in [
        "invalid",
        "https://unit:fixture@localhost/db",
        "postgres://localhost/db",
        "postgres://user@localhost/",
        "postgres://user@localhost/db",
        "postgres://user:password@%2Ftmp/db",
        "postgres://unit:fixture@localhost/db?sslmode=disable",
        "postgres://unit:fixture@localhost/db#fragment",
        "postgres://unit:fixture@localhost/",
        "postgres://unit:@localhost/db",
    ] {
        assert!(
            load(MapEnvironment::from_pairs([(
                "DARKHORSE_DATABASE_URL",
                url
            )]))
            .is_err()
        );
    }
    let settings = load(MapEnvironment::from_pairs([(
        "DARKHORSE_DATABASE_URL",
        "postgres://unit:fixture@localhost/directory",
    )]))
    .unwrap();
    assert_eq!(settings.max_connections, 5);
    assert!(!settings.insecure);
    assert_eq!(settings.url.port(), None);
    let settings = load(MapEnvironment::from_pairs([
        (
            "DARKHORSE_DATABASE_URL",
            "postgres://unit:fixture@localhost/directory",
        ),
        ("DARKHORSE_DATABASE_INSECURE", "true"),
        ("DARKHORSE_DATABASE_POOL_SIZE", "32"),
    ]))
    .unwrap();
    assert!(settings.insecure);
    assert_eq!(settings.max_connections, 32);
    for pool in ["0", "33", "invalid"] {
        assert!(
            load(MapEnvironment::from_pairs([
                (
                    "DARKHORSE_DATABASE_URL",
                    "postgres://unit:fixture@localhost/directory"
                ),
                ("DARKHORSE_DATABASE_POOL_SIZE", pool)
            ]))
            .is_err()
        );
    }
}
