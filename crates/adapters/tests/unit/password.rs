use super::*;
use argon2::{PasswordVerifier, password_hash::phc::PasswordHash};
use darkhorse_application::authentication::{AuthError, PasswordVerification};

#[tokio::test]
async fn verification_pins_policy_and_unknown_users_take_real_hash_work() {
    let worker = PasswordPreparation::default();
    let verifier = hash("test-only passphrase", &[7; 16]).unwrap();
    assert_eq!(
        worker.verify("test-only passphrase", Some(&verifier)).await,
        Ok(true)
    );
    assert_eq!(worker.verify("wrong", Some(&verifier)).await, Ok(false));
    assert_eq!(worker.verify("wrong", None).await, Ok(false));
    for invalid in [
        verifier.replace("m=65536", "m=2147483647"),
        verifier.replace("t=3", "t=2"),
        verifier.replace("argon2id", "argon2i"),
        verifier.replace("v=19", "v=16"),
        "garbage".into(),
        "$argon2id$v=19$m=65536,t=3,p=1$%%%%$%%%%".into(),
        "$argon2id$v=19$m=65536,t=3,p=1$AAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
            .into(),
        "$argon2id$v=19$m=65536,t=3,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAA".into(),
    ] {
        assert_eq!(
            worker.verify("wrong", Some(&invalid)).await,
            Err(AuthError::Unavailable)
        );
    }
    assert_eq!(
        worker.verify(&"x".repeat(513), Some(&verifier)).await,
        Err(AuthError::Denied)
    );
    let _held = worker.slots.clone().acquire_owned().await.unwrap();
    assert_eq!(
        worker.verify("wrong", Some(&verifier)).await,
        Err(AuthError::Unavailable)
    );
}

#[tokio::test]
async fn cancellation_retains_slot_until_blocking_work_finishes() {
    let worker = PasswordPreparation::default();
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let slots = worker.slots.clone();
    let task = tokio::spawn(async move {
        bounded_work(slots, move || {
            started.send(()).unwrap();
            wait.recv().unwrap();
            Ok(())
        })
        .await
    });
    ready.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(worker.slots.available_permits(), 0);
    assert_eq!(
        worker.verify("wrong", None).await,
        Err(AuthError::Unavailable)
    );
    release.send(()).unwrap();
    let permit = tokio::time::timeout(std::time::Duration::from_secs(2), worker.slots.acquire())
        .await
        .unwrap()
        .unwrap();
    drop(permit);
    assert_eq!(worker.slots.available_permits(), 1);
}

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
