use darkhorse_adapters::logout::{configuration, sender::HttpsSender};
use darkhorse_application::logout::delivery::Sender;
use darkhorse_domain::logout::DeliveryResult;
use envbind::{Environment, EnvironmentError};
struct Settings {
    origin: String,
    ca: String,
    allow: bool,
}
impl Environment for Settings {
    fn get(&self, key: &str) -> Result<Option<String>, EnvironmentError> {
        Ok(match key {
            "DARKHORSE_LOGOUT_PRIVATE_DESTINATIONS" => Some(if self.allow {
                format!(r#"{{"{}":["127.0.0.1","::1"]}}"#, self.origin)
            } else {
                "{}".into()
            }),
            "DARKHORSE_LOGOUT_CA_PEM" => Some(self.ca.clone()),
            _ => None,
        })
    }
}
fn sender(origin: &str, ca: &str, allow: bool) -> HttpsSender {
    HttpsSender::new(
        configuration::load(Settings {
            origin: origin.into(),
            ca: ca.into(),
            allow,
        })
        .unwrap(),
    )
}
#[tokio::test]
async fn logout_https_enforces_destination_and_tls_boundaries_without_proxy_or_redirects() {
    let port = std::env::var("DARKHORSE_TEST_LOGOUT_PORT").expect("run make test-logout-https");
    let ca = std::fs::read_to_string(std::env::var("DARKHORSE_TEST_LOGOUT_CA").unwrap()).unwrap();
    let origin = format!("https://localhost:{port}");
    let trusted = sender(&origin, &ca, true);
    let token = "fixture.token+value/signature";
    for (path, expected) in [
        (200, DeliveryResult::Accepted),
        (204, DeliveryResult::Accepted),
        (503, DeliveryResult::Retry),
        (429, DeliveryResult::Retry),
        (400, DeliveryResult::Rejected),
        (302, DeliveryResult::Rejected),
    ] {
        assert_eq!(
            trusted.send(&format!("{origin}/{path}"), token).await,
            expected
        );
    }
    assert_eq!(
        sender(&origin, &ca, false)
            .send(&format!("{origin}/200"), token)
            .await,
        DeliveryResult::Rejected
    );
    assert_eq!(
        sender(&origin, "", true)
            .send(&format!("{origin}/200"), token)
            .await,
        DeliveryResult::Retry
    );
    let wrong = format!("https://127.0.0.1:{port}");
    assert_eq!(
        sender(&wrong, &ca, true)
            .send(&format!("{wrong}/200"), token)
            .await,
        DeliveryResult::Retry
    );
    assert_eq!(
        trusted.send("https://169.254.169.254/", token).await,
        DeliveryResult::Rejected
    );
    assert_eq!(
        trusted
            .send(&format!("{origin}/200"), &"x".repeat(8193))
            .await,
        DeliveryResult::Rejected
    );
    assert_eq!(
        trusted.send(&format!("{origin}/headers"), token).await,
        DeliveryResult::Retry
    );
    let start = std::time::Instant::now();
    assert_eq!(
        trusted.send(&format!("{origin}/slow"), token).await,
        DeliveryResult::Retry
    );
    assert!(start.elapsed() < std::time::Duration::from_secs(7));
}
