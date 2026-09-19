use super::*;
use darkhorse_domain::identity::EmailVerificationId;
#[test]
fn mail_has_one_recipient_fixed_headers_and_fragment_proof_without_redirect_input() {
    let secrets = Secrets::from_hex(&"ab".repeat(32)).unwrap();
    let mut delivery = Delivery {
        created_ms: 100,
        id: EmailVerificationId::from_u128(1).unwrap(),
        attempt: 1,
        email: "one@example.com".into(),
        seed: [3; 32],
    };
    let from = "identity@example.com".parse().unwrap();
    let msg = message(&from, "https://identity.example.com", &secrets, &delivery).unwrap();
    assert_eq!(msg.envelope().to().len(), 1);
    let formatted = String::from_utf8(msg.formatted()).unwrap();
    assert!(formatted.contains("Subject: Verify your Darkhorse email"));
    assert!(formatted.contains("@darkhorse.invalid>"));
    assert!(!formatted.contains("Bcc:"));
    delivery.email = "one@example.com\r\nBcc: two@example.com".into();
    assert!(message(&from, "https://identity.example.com", &secrets, &delivery).is_err());
}
#[test]
fn tls_builder_rejects_invalid_private_ca_without_network_or_environment() {
    let settings = super::super::configuration::load(envbind::MapEnvironment::from_pairs([
        ("DARKHORSE_EMAIL_ENABLED", "true"),
        (
            "DARKHORSE_EMAIL_KEY",
            "abababababababababababababababababababababababababababababababab",
        ),
        ("DARKHORSE_SMTP_HOST", "smtp.example.com"),
        ("DARKHORSE_SMTP_FROM", "identity@example.com"),
    ]))
    .unwrap()
    .unwrap();
    assert!(
        Smtp::build(
            settings,
            "https://identity.example.com".into(),
            Some(b"invalid PEM")
        )
        .is_err()
    );
}
