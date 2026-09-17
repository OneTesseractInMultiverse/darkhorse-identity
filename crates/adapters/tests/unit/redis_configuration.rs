use super::*;
use envbind::MapEnvironment;
fn settings(extra: &[(&str, &str)]) -> Result<RedisSettings, RedisConfigurationError> {
    let mut values = vec![
        (
            "DARKHORSE_REDIS_CACHE_URL",
            "rediss://cache:cache-secret@localhost:63791/0",
        ),
        (
            "DARKHORSE_REDIS_LIMITER_URL",
            "rediss://limiter:limiter-secret@localhost:63792/0",
        ),
    ];
    values.extend_from_slice(extra);
    load(MapEnvironment::from_pairs(values))
}
#[test]
fn requires_distinct_authenticated_roles_and_verified_tls_by_default() {
    assert!(load(MapEnvironment::new()).is_err());
    let good = settings(&[]).unwrap();
    assert_eq!(good.cache.connections, 2);
    assert_eq!(good.limiter.connections, 4);
    assert_eq!(good.cache.timeout_ms, 250);
    for url in [
        "invalid",
        "rediss://cache:%FF@localhost:63791/0",
        "redis://cache:password@localhost:63791/0",
        "rediss://cache@localhost:63791/0",
        "rediss://default:password@localhost:63791/0",
        "rediss://cache:password@localhost:63791/1",
        "rediss://cache:password@localhost:63791/0?protocol=resp3",
        "rediss://cache:password@localhost:63791/0#insecure",
        "rediss://c%61che:password@localhost:63791/0",
        "rediss://cache:password@%2Ftmp:63791/0",
    ] {
        assert!(
            settings(&[("DARKHORSE_REDIS_CACHE_URL", url)]).is_err(),
            "{url}"
        );
    }
    assert!(
        settings(&[
            (
                "DARKHORSE_REDIS_CACHE_URL",
                "redis://cache:password@localhost:63791/0"
            ),
            ("DARKHORSE_REDIS_INSECURE", "true")
        ])
        .is_ok()
    );
    assert!(
        settings(&[(
            "DARKHORSE_REDIS_CACHE_URL",
            "rediss://cache:another-secret@localhost:63792/0"
        )])
        .is_err()
    );
    assert!(
        settings(&[(
            "DARKHORSE_REDIS_CACHE_URL",
            "rediss://cache:limiter-secret@localhost:63791/0"
        )])
        .is_err()
    );
    assert!(
        settings(&[(
            "DARKHORSE_REDIS_CACHE_URL",
            "rediss://cache:limiter%2Dsecret@localhost:63791/0"
        )])
        .is_err()
    );
    for (name, value) in [
        ("DARKHORSE_REDIS_CACHE_CONNECTIONS", "0"),
        ("DARKHORSE_REDIS_LIMITER_CONNECTIONS", "17"),
        ("DARKHORSE_REDIS_TIMEOUT_MS", "9"),
        ("DARKHORSE_REDIS_TIMEOUT_MS", "1001"),
    ] {
        assert!(settings(&[(name, value)]).is_err());
    }
    assert!(
        settings(&[
            ("DARKHORSE_REDIS_CACHE_CONNECTIONS", "16"),
            ("DARKHORSE_REDIS_TIMEOUT_MS", "10")
        ])
        .is_ok()
    );
}

#[test]
fn private_trust_is_explicit_bounded_and_cannot_be_combined_with_plaintext() {
    let pem = "-----BEGIN CERTIFICATE-----\nfixture\n-----END CERTIFICATE-----\n";
    assert_eq!(
        settings(&[("DARKHORSE_REDIS_LIMITER_CA_PEM", pem)])
            .unwrap()
            .limiter
            .ca_pem
            .as_deref(),
        Some(pem)
    );
    for value in [
        "garbage",
        "-----BEGIN CERTIFICATE-----\nmissing end",
        &"a".repeat(16385),
    ] {
        assert!(settings(&[("DARKHORSE_REDIS_LIMITER_CA_PEM", value)]).is_err());
    }
    assert!(
        settings(&[
            ("DARKHORSE_REDIS_INSECURE", "true"),
            (
                "DARKHORSE_REDIS_LIMITER_URL",
                "redis://limiter:secret@localhost:63792/0"
            ),
            ("DARKHORSE_REDIS_LIMITER_CA_PEM", pem)
        ])
        .is_err()
    );
    assert!(settings(&[("DARKHORSE_REDIS_CACHE_CA_PEM", &"x".repeat(16385))]).is_err());
}
