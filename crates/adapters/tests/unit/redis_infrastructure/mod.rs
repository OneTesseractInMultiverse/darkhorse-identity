use super::*;
#[test]
fn aliases_of_one_process_fail_isolation_and_one_outage_does_not_hide_the_other_role() {
    let id = Identity {
        run_id: "a".repeat(40),
        memory_limit_bytes: 1,
        used_memory_bytes: 0,
    };
    let status = separate_roles(Ok(id.clone()), Ok(id.clone()));
    assert_eq!(status.cache, Err(ProbeFailure::SharedInstance));
    assert_eq!(status.limiter, Err(ProbeFailure::SharedInstance));
    let status = separate_roles(Err(ProbeFailure::Unavailable), Ok(id.clone()));
    assert_eq!(status.cache, Err(ProbeFailure::Unavailable));
    assert_eq!(status.limiter, Ok(id));
}

#[tokio::test]
async fn saturated_pool_rejects_immediately_without_opening_another_connection() {
    let pool = Pool::new(Endpoint {
        url: url::Url::parse("rediss://unit:fixture@localhost/0").unwrap(),
        connections: 2,
        timeout_ms: 250,
        ca_pem: None,
    })
    .unwrap();
    let _guards: Vec<_> = pool
        .slots
        .iter()
        .map(|slot| slot.try_lock().unwrap())
        .collect();
    assert_eq!(
        pool.inspect(Role::Limiter).await,
        Err(ProbeFailure::Unavailable)
    );
}
