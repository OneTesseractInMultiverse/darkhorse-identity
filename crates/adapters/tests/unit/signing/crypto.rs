use super::*;
use base64::engine::general_purpose::STANDARD;
mod fixture;
fn key() -> KeyPair {
    parse(&STANDARD.decode(fixture::PKCS8).unwrap()).unwrap()
}
fn wrap() -> WrapKey {
    WrapKey::from_hex(&"13".repeat(32)).unwrap()
}
#[test]
fn wrapped_keys_authenticate_issuer_key_identity_ciphertext_and_public_projection() {
    let key = key();
    let wrap = wrap();
    let mut stored = wrap_key("https://issuer.example", &wrap, &key, [7; 12]).unwrap();
    assert_eq!(
        public(&unwrap("https://issuer.example", &wrap, &stored).unwrap()),
        public(&key)
    );
    assert!(unwrap("https://other.example", &wrap, &stored).is_err());
    assert!(
        unwrap(
            "https://issuer.example",
            &WrapKey::from_hex(&"14".repeat(32)).unwrap(),
            &stored
        )
        .is_err()
    );
    stored.ciphertext[0] ^= 1;
    assert!(unwrap("https://issuer.example", &wrap, &stored).is_err());
    stored.ciphertext[0] ^= 1;
    stored.public.n.push('A');
    assert!(unwrap("https://issuer.example", &wrap, &stored).is_err());
    assert!(parse(&[]).is_err());
    assert!(parse(&[1; 8193]).is_err());
    assert!(parse(&[1; 100]).is_err());
    assert!(WrapKey::from_hex(&"00".repeat(32)).is_err());
    assert!(WrapKey::from_hex("bad").is_err());
    assert_ne!(
        wrap.fingerprint(),
        WrapKey::from_hex(&"14".repeat(32)).unwrap().fingerprint()
    );
}
#[test]
fn rs256_public_material_interoperates_with_an_independent_verifier() {
    let key = key();
    let public = public(&key);
    assert_eq!(public.kid.len(), 43);
    assert_eq!(public.e, "AQAB");
    let message = b"bounded protocol signing interoperability fixture";
    let mut signature = vec![0; key.public_modulus_len()];
    key.sign(
        &aws_lc_rs::signature::RSA_PKCS1_SHA256,
        &aws_lc_rs::rand::SystemRandom::new(),
        message,
        &mut signature,
    )
    .unwrap();
    let verifier = ring::signature::RsaPublicKeyComponents {
        n: URL_SAFE_NO_PAD.decode(&public.n).unwrap(),
        e: URL_SAFE_NO_PAD.decode(&public.e).unwrap(),
    };
    verifier
        .verify(
            &ring::signature::RSA_PKCS1_2048_8192_SHA256,
            message,
            &signature,
        )
        .unwrap();
    assert!(
        verifier
            .verify(
                &ring::signature::RSA_PKCS1_2048_8192_SHA256,
                b"changed",
                &signature
            )
            .is_err()
    );
}
