use super::*;
fn delivery(kind: Kind, locale: Locale) -> Delivery<()> {
    Delivery {
        id: (),
        created_ms: 100,
        expires_ms: 100
            + match kind {
                Kind::Verification => 900_000,
                Kind::Invitation => 86_400_000,
            },
        attempt: 1,
        email: "Δ@example.com".into(),
        seed: [1; 32],
        locale,
        template_version: 1,
    }
}
#[test]
fn fixed_languages_preserve_canonical_purpose_links_and_actual_lifetimes() {
    for kind in [Kind::Verification, Kind::Invitation] {
        let (prefix, path) = match kind {
            Kind::Verification => ("ev1_", "security/email"),
            Kind::Invitation => ("iv1_", "invitation"),
        };
        let token = format!("{prefix}{}", "ab".repeat(32));
        for locale in [Locale::English, Locale::Spanish] {
            let mut job = delivery(kind, locale);
            let text = render("https://identity.example.com", &token, &job, kind).unwrap();
            assert!(text.body.contains(&format!(
                "https://identity.example.com/{path}#token={token}&lang={}",
                crate::localization::tag(locale)
            )));
            assert!(!text.body.contains(&job.email));
            assert!(text.body.len() < 4096);
            assert!(!text.subject.contains(['\r', '\n']));
            assert!(text.body.contains(match kind {
                Kind::Verification => "15",
                Kind::Invitation => "24",
            }));
            job.attempt = 5;
            assert_eq!(
                text.body,
                render("https://identity.example.com", &token, &job, kind)
                    .unwrap()
                    .body
            );
            job.expires_ms += 1;
            assert!(render("https://identity.example.com", &token, &job, kind).is_err());
        }
    }
}
#[test]
fn unsafe_origins_tokens_versions_and_expiries_fail_without_output() {
    let mut job = delivery(Kind::Verification, Locale::English);
    let token = format!("ev1_{}", "ab".repeat(32));
    for origin in [
        "http://identity.example.com",
        "https://identity.example.com/",
        "https://name:password@identity.example.com",
        "https://identity.example.com?next=evil",
        "https://identity.example.com\r\nBcc:evil",
        "https://identity.example.com/#evil",
    ] {
        assert!(render(origin, &token, &job, Kind::Verification).is_err());
    }
    for invalid in [
        format!("{token}&evil=1"),
        format!("{token}\r\nInjected"),
        "iv1_".to_owned() + &"ab".repeat(32),
        "x".repeat(8192),
    ] {
        assert!(
            render(
                "https://identity.example.com",
                &invalid,
                &job,
                Kind::Verification
            )
            .is_err()
        );
    }
    for version in [0, 2, u16::MAX] {
        job.template_version = version;
        assert!(
            render(
                "https://identity.example.com",
                &token,
                &job,
                Kind::Verification
            )
            .is_err()
        );
    }
    job.template_version = 1;
    job.expires_ms = 0;
    assert!(
        render(
            "https://identity.example.com",
            &token,
            &job,
            Kind::Verification
        )
        .is_err()
    );
}
