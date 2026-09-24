use super::*;
#[test]
fn account_limiter_uses_one_connection_without_changing_security_configuration() {
    let settings = crate::redis_configuration::load(envbind::MapEnvironment::from_pairs([
        (
            "DARKHORSE_REDIS_CACHE_URL",
            "rediss://cache:cache-secret@localhost:63791/0",
        ),
        (
            "DARKHORSE_REDIS_LIMITER_URL",
            "rediss://limiter:limiter-secret@localhost:63792/0",
        ),
        ("DARKHORSE_REDIS_LIMITER_CONNECTIONS", "16"),
    ]))
    .unwrap();
    let limited = limit_redis(settings);
    assert_eq!(limited.limiter.connections, 1);
    assert_eq!(limited.cache.connections, 2);
    assert_eq!(limited.limiter.timeout_ms, 250);
    assert_eq!(limited.limiter.url.scheme(), "rediss");
}
