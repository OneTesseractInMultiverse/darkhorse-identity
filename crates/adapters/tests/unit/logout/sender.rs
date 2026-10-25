use super::*;
#[test]
fn redirects_and_client_errors_are_terminal_and_temporary_failures_retry() {
    for code in [200, 204] {
        assert_eq!(status(code), DeliveryResult::Accepted);
    }
    for code in [408, 429, 500, 502, 503, 599] {
        assert_eq!(status(code), DeliveryResult::Retry);
    }
    for code in [100, 201, 202, 301, 302, 307, 308, 400, 401, 403, 404, 405] {
        assert_eq!(status(code), DeliveryResult::Rejected);
    }
}

#[tokio::test]
async fn saturated_resolver_admission_fails_before_starting_network_work() {
    let sender = HttpsSender::new(Settings {
        policy: Policy::default(),
        certificates: Vec::new(),
    });
    let _held = sender.dns_slots.acquire_many(4).await.unwrap();
    assert_eq!(
        sender
            .send("https://fixture.invalid/logout", "fixture")
            .await,
        DeliveryResult::Retry
    );
    assert_eq!(
        sender
            .send("http://fixture.invalid/logout", "fixture")
            .await,
        DeliveryResult::Rejected
    );
}
