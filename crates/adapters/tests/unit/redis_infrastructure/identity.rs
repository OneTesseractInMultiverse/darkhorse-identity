use super::*;
fn server() -> String {
    format!("redis_mode:standalone\r\nrun_id:{}\r\n", "a".repeat(40))
}
#[test]
fn identity_requires_standalone_primary_bounded_memory_and_role_policy() {
    let memory = "maxmemory:67108864\nused_memory:1048576\nmaxmemory_policy:noeviction\n";
    let good = validate(Role::Limiter, "PONG", &server(), memory, "role:master\n").unwrap();
    assert_eq!(good.run_id, "a".repeat(40));
    assert_eq!(good.memory_limit_bytes, 67108864);
    assert_eq!(good.used_memory_bytes, 1048576);
    assert!(
        validate(
            Role::Cache,
            "PONG",
            &server(),
            &memory.replace("noeviction", "allkeys-lru"),
            "role:master"
        )
        .is_ok()
    );
    for (pong, s, m, r) in [
        (
            "invalid".to_owned(),
            server(),
            memory.to_owned(),
            "role:master".to_owned(),
        ),
        (
            "PONG".to_owned(),
            server().replace("standalone", "cluster"),
            memory.to_owned(),
            "role:master".to_owned(),
        ),
        (
            "PONG".to_owned(),
            server(),
            memory.to_owned(),
            "role:slave".to_owned(),
        ),
        (
            "PONG".to_owned(),
            server().replace(&"a".repeat(40), &"g".repeat(40)),
            memory.to_owned(),
            "role:master".to_owned(),
        ),
        (
            "PONG".to_owned(),
            server(),
            memory.replace("noeviction", "allkeys-lru"),
            "role:master".to_owned(),
        ),
        (
            "PONG".to_owned(),
            server(),
            memory.replace("67108864", "0"),
            "role:master".to_owned(),
        ),
        (
            "PONG".to_owned(),
            server(),
            memory.replace("1048576", "invalid"),
            "role:master".to_owned(),
        ),
        (
            "PONG".to_owned(),
            server(),
            memory.replace("1048576", "67108865"),
            "role:master".to_owned(),
        ),
        (
            "PONG".to_owned(),
            server(),
            format!("{memory}maxmemory:1\n"),
            "role:master".to_owned(),
        ),
        (
            "PONG".to_owned(),
            "a".repeat(16385),
            memory.to_owned(),
            "role:master".to_owned(),
        ),
    ] {
        assert_eq!(
            validate(Role::Limiter, &pong, &s, &m, &r),
            Err(ProbeFailure::UnsafeConfiguration)
        );
    }
}
