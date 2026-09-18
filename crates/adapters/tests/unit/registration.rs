use super::*;
#[test]
fn callbacks_are_canonical_https_and_exact() {
    let valid = "https://app.example:8443/callback?tenant=a%20b";
    assert!(redirects(vec![valid.into()]).unwrap().allows(valid));
    for invalid in [
        "http://app.example/cb",
        "https://app.example",
        "https://APP.example/cb",
        "https://app.example:443/cb",
        "https://user:pass@app.example/cb",
        "https://app.example/cb#",
        "https://*.example/cb",
        "https://app.example/cb*",
        "https://app.example/cb%",
        "https://app.example/cb%2g",
        "https://app.example/c b",
        "https://app.example\\cb",
        "https://app.example/../cb",
        "https://éxample.com/cb",
        "javascript:alert(1)",
        "",
        "https:///app.example/cb",
    ] {
        assert!(redirects(vec![invalid.into()]).is_err(), "{invalid}");
    }
    assert!(redirects(vec![]).is_err());
}
#[test]
fn secret_verifiers_are_deterministic_and_purpose_separated() {
    let secret = from_bytes([7; 32], ClientSecretId::from_u128(1).unwrap());
    assert_eq!(secret.value.len(), 64);
    assert_eq!(
        secret.verifier.digest,
        secret_digest(&secret.value).unwrap()
    );
    assert_ne!(
        secret.verifier.digest,
        crate::session_secret::digest(&secret.value).unwrap()
    );
    assert!(secret_digest("bad").is_err());
}
