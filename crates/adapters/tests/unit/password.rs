use super::*;
use argon2::{PasswordVerifier, password_hash::phc::PasswordHash};

#[test]
fn production_hash_parameters_verify_and_wrong_password_fails() {
    let mut material = [0; 48];
    for (index, byte) in material.iter_mut().enumerate() {
        *byte = index as u8;
    }
    let prepared = prepare_with_material("test-only passphrase", material).unwrap();
    assert_ne!(
        prepared.principal_id.as_u128(),
        prepared.credential_id.as_u128()
    );
    assert!(
        prepared
            .verifier
            .starts_with("$argon2id$v=19$m=65536,t=3,p=1$")
    );
    let parsed = PasswordHash::new(&prepared.verifier).unwrap();
    assert!(
        Argon2::default()
            .verify_password(b"test-only passphrase", &parsed)
            .is_ok()
    );
    assert!(
        Argon2::default()
            .verify_password(b"wrong password", &parsed)
            .is_err()
    );
    assert_eq!(
        hash("test-only passphrase", b"short"),
        Err(BootstrapError::SecretPreparation)
    );
}

#[tokio::test]
async fn admission_failure_does_not_schedule_or_consume_entropy() {
    let preparation = PasswordPreparation::default();
    let _permit = preparation.slots.clone().acquire_owned().await.unwrap();
    assert!(matches!(
        preparation.prepare("test-only passphrase").await,
        Err(BootstrapError::SecretPreparation)
    ));
}
