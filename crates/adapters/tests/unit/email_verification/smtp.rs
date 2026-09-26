use super::*;
use darkhorse_domain::identity::EmailVerificationId;
#[test]
fn mail_has_one_recipient_fixed_headers_and_fragment_proof_without_redirect_input() {
    let secrets = Secrets::from_hex(&"ab".repeat(32)).unwrap();
    let mut delivery = Delivery {
        created_ms: 100,
        expires_ms: 900100,
        locale: darkhorse_domain::localization::Locale::English,
        template_version: 0,
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
#[test]
fn invitation_mail_has_a_distinct_stable_message_and_one_recipient() {
    let secrets = Secrets::from_hex(&"ab".repeat(32)).unwrap();
    let mut delivery = darkhorse_application::invitations::Delivery {
        created_ms: 100,
        expires_ms: 900100,
        locale: darkhorse_domain::localization::Locale::English,
        template_version: 0,
        id: darkhorse_domain::identity::InvitationId::from_u128(1).unwrap(),
        attempt: 1,
        email: "new@example.com".into(),
        seed: [3; 32],
    };
    let from = "identity@example.com".parse().unwrap();
    let first =
        invitation_message(&from, "https://identity.example.com", &secrets, &delivery).unwrap();
    assert_eq!(first.envelope().to().len(), 1);
    delivery.attempt = 2;
    assert_eq!(
        first.formatted(),
        invitation_message(&from, "https://identity.example.com", &secrets, &delivery)
            .unwrap()
            .formatted()
    );
    let formatted = String::from_utf8(first.formatted()).unwrap();
    assert!(formatted.contains("Subject: Your Darkhorse invitation"));
    assert!(formatted.contains("<invitation-"));
    assert!(!formatted.contains("ev1_"));
    delivery.email = "new@example.com\r\nBcc: other@example.com".into();
    assert!(
        invitation_message(&from, "https://identity.example.com", &secrets, &delivery).is_err()
    );
}
#[test]
fn spanish_delivery_is_pinned_bounded_and_keeps_proof_in_fragment() {
    let secrets = Secrets::from_hex(&"ab".repeat(32)).unwrap();
    let from = "identity@example.com".parse().unwrap();
    let mut delivery = Delivery {
        created_ms: 100,
        expires_ms: 900_100,
        locale: darkhorse_domain::localization::Locale::Spanish,
        template_version: 1,
        id: EmailVerificationId::from_u128(1).unwrap(),
        attempt: 1,
        email: "one@example.com".into(),
        seed: [3; 32],
    };
    let first = message(&from, "https://identity.example.com", &secrets, &delivery).unwrap();
    delivery.attempt = 4;
    assert_eq!(
        first.formatted(),
        message(&from, "https://identity.example.com", &secrets, &delivery)
            .unwrap()
            .formatted()
    );
    let formatted = String::from_utf8(first.formatted()).unwrap();
    assert!(formatted.contains("Subject: Verifica tu correo de Darkhorse"));
    assert!(formatted.contains("15 minutos"));
    assert!(formatted.len() < 4096);
    delivery.template_version = 99;
    assert!(message(&from, "https://identity.example.com", &secrets, &delivery).is_err());
}
