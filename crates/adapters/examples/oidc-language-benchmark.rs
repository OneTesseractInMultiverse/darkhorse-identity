//! Bounded hint processing only; no HTTP, cookie, SQL, authorization or capacity claim.
use darkhorse_adapters::locale_hint::ui_locales;
use std::{hint::black_box, time::Instant};

fn sample(value: Option<&str>, iterations: u32) -> u128 {
    let started = Instant::now();
    for _ in 0..iterations {
        let _ = black_box(ui_locales(black_box(value)));
    }
    started.elapsed().as_nanos()
}

fn observations() -> Vec<serde_json::Value> {
    let maximum = ["es-Latn-CR"; 16].join(" ");
    let cases = [
        ("absent", None),
        ("supported", Some("es-CR es en")),
        ("unsupported", Some("de-DE fr-CA")),
        ("sixteen-tags", Some(maximum.as_str())),
        ("malformed-after-supported", Some("en x-a--b")),
    ];
    let mut results = Vec::new();
    for repeat in 0..5 {
        for offset in 0..cases.len() {
            let (name, value) = cases[(offset + repeat) % cases.len()];
            let _ = sample(value, 1000);
            results.push(serde_json::json!({
                "repeat": repeat, "case": name, "inputBytes": value.map_or(0, str::len),
                "iterations": 100_000, "elapsedNs": sample(value, 100_000),
            }));
        }
    }
    results
}

fn main() {
    let observations = observations();
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "schema": 1, "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
        "profile": "release", "parser": "language-tags 0.3.2",
        "scope": "In-process bounded ui_locales validation/resolution; excludes HTTP, cookies, SQL, authentication and authorization. No production throughput claim.",
        "observations": observations,
    })).unwrap());
}
