use super::operator_directory::{Allow, Password};
use super::*;
use darkhorse_application::{
    operator_accounts,
    registration::{ClientAuthenticationStore, Entropy, NewSecret, SecretVerifier},
};
use darkhorse_domain::{
    identity::{ApplicationId, ClientId, ClientSecretId, OperationId},
    operator_accounts::Error,
    operator_client_rotation::Request,
    operator_client_secrets::Target,
    registration::RegistrationError,
};
use sha2::{Digest, Sha256};
use std::num::NonZeroU128;
use uuid::Uuid;

struct FixedEntropy {
    id: u128,
    value: String,
}

impl Entropy for FixedEntropy {
    fn identifier(&self) -> Result<NonZeroU128, RegistrationError> {
        NonZeroU128::new(self.id).ok_or(RegistrationError::Unavailable)
    }

    fn secret(&self) -> Result<NewSecret, RegistrationError> {
        let id = ClientSecretId::from_u128(self.id).map_err(|_| RegistrationError::Unavailable)?;
        Ok(NewSecret {
            value: self.value.clone(),
            verifier: SecretVerifier {
                id,
                digest: digest(&self.value),
            },
        })
    }
}

fn target() -> Target {
    Target {
        application: ApplicationId::from_u128(16).unwrap(),
        client: ClientId::from_u128(32).unwrap(),
    }
}

fn digest(value: &str) -> [u8; 32] {
    Sha256::new()
        .chain_update(b"darkhorse:client-secret:v1\0")
        .chain_update(value.as_bytes())
        .finalize()
        .into()
}

fn request(revision: u64, overlap_seconds: u16) -> Request {
    Request::new(
        target(),
        revision,
        overlap_seconds,
        "Integration fixture rotation",
    )
    .unwrap()
}

async fn run(
    db: &Database,
    revision: u64,
    overlap_seconds: u16,
    id: u128,
) -> Result<darkhorse_adapters::postgres::operator_client_rotation::RotatedSecret, Error> {
    operator_accounts::run(
        &db.store.operator_client_rotation(FixedEntropy {
            id: 1000 + id,
            value: format!("{id:064x}"),
        }),
        &Allow,
        &Password(true),
        OperationId::from_u128(Uuid::from_u128(id).as_u128()).unwrap(),
        request(revision, overlap_seconds),
        "one@example.com",
        "source-only passphrase",
    )
    .await
}

async fn fixture() -> Database {
    oidc::fixture().await
}

#[tokio::test]
async fn rotation_uses_registration_policy_applies_overlap_and_returns_the_secret_once() {
    let db = fixture().await;
    let original = "1".repeat(64);
    sqlx::query(
        "INSERT INTO oauth_client_secrets(id,client_id,verifier,created_ms) VALUES($1,$2,$3,0)",
    )
    .bind(Uuid::from_u128(501))
    .bind(Uuid::from_u128(target().client.as_u128()))
    .bind(digest(&original).as_slice())
    .execute(&db.pool)
    .await
    .unwrap();

    let first = run(&db, 0, 60, 11).await.unwrap();
    let first_secret = first.secret.to_string();
    let first_secret_id = first.secret_id;
    assert_eq!(first.revision, 1);
    assert!(
        db.store
            .authenticate_client(target().client, digest(&original))
            .await
            .is_ok(),
        "the previous credential remains valid during the requested overlap"
    );
    assert!(
        db.store
            .authenticate_client(target().client, digest(&first_secret))
            .await
            .is_ok()
    );

    let expires: Option<i64> =
        sqlx::query_scalar("SELECT expires_ms FROM oauth_client_secrets WHERE id=$1")
            .bind(Uuid::from_u128(501))
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert!(expires.is_some());
    let audit: String = sqlx::query_scalar("SELECT row_to_json(a)::text FROM operator_client_secret_rotation_audit a WHERE operation_id=$1")
        .bind(Uuid::from_u128(11))
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert!(audit.contains("written"));
    assert!(audit.contains(&Uuid::from_u128(first_secret_id.as_u128()).to_string()));
    assert!(!audit.contains(&first_secret));
    let verifier_hex: String = digest(&first_secret)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert!(!audit.contains(&verifier_hex));

    let second = run(&db, 1, 0, 12).await.unwrap();
    let second_secret = second.secret.to_string();
    assert_eq!(second.revision, 2);
    assert_ne!(second.secret_id, first_secret_id);
    assert!(
        db.store
            .authenticate_client(target().client, digest(&first_secret))
            .await
            .is_err(),
        "zero overlap immediately retires the previous current secret"
    );
    assert!(
        db.store
            .authenticate_client(target().client, digest(&second_secret))
            .await
            .is_ok()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM registration_audit WHERE event='secret_rotated'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        2
    );
}

#[tokio::test]
async fn concurrent_rotations_at_one_revision_have_one_winner_and_one_conflict() {
    let db = fixture().await;
    let (first, second) = tokio::join!(run(&db, 0, 0, 21), run(&db, 0, 0, 22));
    let outcomes = [first, second];
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Err(Error::Conflict)))
            .count(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT revision FROM oauth_clients WHERE id=$1")
            .bind(Uuid::from_u128(target().client.as_u128()))
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM operator_client_secret_rotation_audit WHERE result='written'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM operator_client_secret_rotation_audit WHERE result='conflict'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        1
    );
}

#[tokio::test]
async fn required_rotation_audit_failure_rolls_back_secret_and_revision() {
    let db = fixture().await;
    sqlx::raw_sql("CREATE FUNCTION reject_secret_rotation_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture'; END $$; CREATE TRIGGER reject_secret_rotation_audit BEFORE INSERT ON operator_client_secret_rotation_audit FOR EACH ROW EXECUTE FUNCTION reject_secret_rotation_audit();")
        .execute(&db.pool)
        .await
        .unwrap();

    assert!(matches!(run(&db, 0, 30, 31).await, Err(Error::Unavailable)));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT revision FROM oauth_clients WHERE id=$1")
            .bind(Uuid::from_u128(target().client.as_u128()))
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM oauth_client_secrets WHERE client_id=$1"
        )
        .bind(Uuid::from_u128(target().client.as_u128()))
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM registration_audit WHERE event='secret_rotated'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        0
    );
}
