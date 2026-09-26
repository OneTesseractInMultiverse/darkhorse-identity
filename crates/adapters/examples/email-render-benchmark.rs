//! Pure template timing only; excludes credentials, MIME, SMTP, storage and network.
use darkhorse_adapters::{
    email_verification::{invitation_digest, token_digest},
    localization,
};
use darkhorse_application::email_delivery::Delivery;
use darkhorse_domain::localization::Locale;
use std::{hint::black_box, time::Instant};
#[path = "../src/email_verification/render.rs"]
mod render;
fn fixture(kind: render::Kind, locale: Locale) -> Delivery<()> {
    Delivery {
        id: (),
        created_ms: 100,
        expires_ms: 100
            + match kind {
                render::Kind::Verification => darkhorse_domain::email_verification::LIFETIME_MS,
                render::Kind::Invitation => darkhorse_domain::invitations::LIFETIME_MS,
            },
        email: "fixture@example.com".into(),
        seed: [1; 32],
        attempt: 1,
        locale,
        template_version: 1,
    }
}
fn run(kind: render::Kind, locale: Locale, iterations: u32) -> (u128, usize, usize) {
    let job = fixture(kind, locale);
    let token = match kind {
        render::Kind::Verification => "ev1_",
        render::Kind::Invitation => "iv1_",
    }
    .to_owned()
        + &"ab".repeat(32);
    let one = render::render("https://identity.example.com", &token, &job, kind).unwrap();
    let started = Instant::now();
    for _ in 0..iterations {
        black_box(
            render::render(
                black_box("https://identity.example.com"),
                black_box(&token),
                black_box(&job),
                kind,
            )
            .unwrap(),
        );
    }
    (
        started.elapsed().as_nanos(),
        one.subject.len(),
        one.body.len(),
    )
}
fn samples() -> Vec<serde_json::Value> {
    let mut samples = Vec::new();
    for repeat in 0..5 {
        for (name, kind) in [
            ("verification", render::Kind::Verification),
            ("invitation", render::Kind::Invitation),
        ] {
            let locales = if repeat % 2 == 0 {
                [Locale::English, Locale::Spanish]
            } else {
                [Locale::Spanish, Locale::English]
            };
            for locale in locales {
                let _ = run(kind, locale, 1000);
                let (elapsed_ns, subject_bytes, body_bytes) = run(kind, locale, 100_000);
                samples.push(serde_json::json!({"repeat":repeat,"kind":name,"locale":localization::tag(locale),"iterations":100_000,"elapsedNs":elapsed_ns,"subjectBytes":subject_bytes,"bodyBytes":body_bytes}));
            }
        }
    }
    samples
}
fn main() {
    let samples = samples();
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"schema":1,"scope":"In-process version-1 plaintext rendering and input validation; no MIME, HMAC derivation, SQL, SMTP, network or capacity claim.","os":std::env::consts::OS,"arch":std::env::consts::ARCH,"profile":"release","observations":samples})).unwrap());
}
