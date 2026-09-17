use super::*;
#[test]
fn diagnostics_expose_only_role_health_and_public_process_metadata() {
    let ok = Ok(Identity {
        run_id: "a".repeat(40),
        memory_limit_bytes: 64,
        used_memory_bytes: 1,
    });
    assert_eq!(
        project(&ok),
        serde_json::json!({"connection":"reachable","run_id":"a".repeat(40),"memory_limit_bytes":64,"used_memory_bytes":1})
    );
    assert_eq!(result(&ok, &ok), Ok(()));
    for (failure, expected) in [
        (ProbeFailure::Unavailable, "unavailable"),
        (ProbeFailure::UnsafeConfiguration, "unsafe_configuration"),
        (ProbeFailure::SharedInstance, "shared_instance"),
    ] {
        let failed = Err(failure);
        assert_eq!(project(&failed), serde_json::json!({"connection":expected}));
        assert!(result(&failed, &ok).is_err());
        assert!(result(&ok, &failed).is_err());
    }
}
