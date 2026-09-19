use darkhorse_adapters::email_verification::{configuration, smtp::Smtp};
use darkhorse_application::email_verification::{Delivery, DeliveryResult, EmailDelivery};
use darkhorse_domain::identity::EmailVerificationId;
#[tokio::test]
async fn smtp_requires_trusted_chain_correct_hostname_and_valid_credentials() {
    let load = || {
        configuration::load(envbind::ProcessEnvironment)
            .expect("use make test-browser for disposable TLS SMTP")
            .expect("email test configuration required")
    };
    let delivery = Delivery {
        id: EmailVerificationId::from_u128(1).unwrap(),
        created_ms: 1,
        attempt: 1,
        email: "fixture@example.com".into(),
        seed: [1; 32],
    };
    let trusted = Smtp::prepare(load(), "https://identity.example.com".into()).unwrap();
    assert_eq!(trusted.deliver(&delivery).await, DeliveryResult::Accepted);
    let mut no_ca = load();
    no_ca.ca_file = None;
    let untrusted = Smtp::prepare(no_ca, "https://identity.example.com".into()).unwrap();
    assert_eq!(untrusted.deliver(&delivery).await, DeliveryResult::Retry);
    let mut wrong_host = load();
    wrong_host.host = "127.0.0.1".into();
    let wrong_host = Smtp::prepare(wrong_host, "https://identity.example.com".into()).unwrap();
    assert_eq!(wrong_host.deliver(&delivery).await, DeliveryResult::Retry);
    let mut wrong_credentials = load();
    wrong_credentials.password = zeroize::Zeroizing::new("wrong-credential".into());
    let denied = Smtp::prepare(wrong_credentials, "https://identity.example.com".into()).unwrap();
    assert_eq!(denied.deliver(&delivery).await, DeliveryResult::Rejected);
    let bad_recipient = Delivery {
        email: "fixture@example.com\r\nBcc: bad@example.com".into(),
        ..delivery
    };
    assert_eq!(
        trusted.deliver(&bad_recipient).await,
        DeliveryResult::Rejected
    );
    use darkhorse_application::invitations::InvitationDelivery;
    let bad_invitation = darkhorse_application::invitations::Delivery {
        id: darkhorse_domain::identity::InvitationId::from_u128(1).unwrap(),
        created_ms: 1,
        attempt: 1,
        email: "fixture@example.com\r\nBcc: bad@example.com".into(),
        seed: [1; 32],
    };
    assert_eq!(
        trusted.deliver_invitation(&bad_invitation).await,
        DeliveryResult::Rejected
    );
    let mut missing_ca = load();
    missing_ca.ca_file = Some("/nonexistent/email-test-ca.pem".into());
    assert!(Smtp::prepare(missing_ca, "https://identity.example.com".into()).is_err());
}
