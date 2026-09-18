use super::*;
use envbind::MapEnvironment;
#[test]
fn enablement_requires_explicit_canonical_key_and_redacts_errors() {
    assert!(load(MapEnvironment::new()).unwrap().is_none());
    for key in ["", "secret-invalid", &"0".repeat(64), &"f".repeat(65)] {
        assert_eq!(
            load(MapEnvironment::from_pairs([
                ("DARKHORSE_LOGIN_ENABLED", "true"),
                ("DARKHORSE_LOGIN_LIMIT_KEY", key)
            ]))
            .err(),
            Some(AuthenticationConfigurationError)
        );
    }
    let s = load(MapEnvironment::from_pairs([
        ("DARKHORSE_LOGIN_ENABLED", "true"),
        ("DARKHORSE_LOGIN_LIMIT_KEY", &"ab".repeat(32)),
    ]))
    .unwrap()
    .unwrap();
    assert_eq!(s.key, [0xab; 32]);
    assert!(
        load(MapEnvironment::from_pairs([(
            "DARKHORSE_LOGIN_ENABLED",
            "invalid"
        )]))
        .is_err()
    );
}
