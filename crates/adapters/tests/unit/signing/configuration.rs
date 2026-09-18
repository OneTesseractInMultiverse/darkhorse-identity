use super::*;
use envbind::MapEnvironment;
#[test]
fn wrapping_keys_are_explicit_canonical_and_redacted() {
    assert!(load(MapEnvironment::new()).unwrap().is_none());
    for value in ["", &"0".repeat(64), &"A".repeat(64), &"a".repeat(65)] {
        assert!(
            load(MapEnvironment::from_pairs([
                ("DARKHORSE_PROVIDER_ENABLED", "true"),
                ("DARKHORSE_SIGNING_WRAP_KEY", value)
            ]))
            .is_err()
        );
    }
    assert!(
        load(MapEnvironment::from_pairs([(
            "DARKHORSE_PROVIDER_ENABLED",
            "invalid"
        )]))
        .is_err()
    );
    let key = load(MapEnvironment::from_pairs([
        ("DARKHORSE_PROVIDER_ENABLED", "true"),
        ("DARKHORSE_SIGNING_WRAP_KEY", &"ab".repeat(32)),
    ]))
    .unwrap()
    .unwrap();
    assert_eq!(key.bytes(), [0xab; 32]);
    assert_eq!(
        key.fingerprint(),
        WrapKey::from_hex(&"ab".repeat(32)).unwrap().fingerprint()
    );
}
