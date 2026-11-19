//! Password hashing uses explicit salt; entropy and bounded worker scheduling are effects.
use argon2::{
    Algorithm, Argon2, Params, PasswordHasher, PasswordVerifier, Version,
    password_hash::phc::PasswordHash,
};
use darkhorse_application::authentication::{AuthError, PasswordVerification};
use darkhorse_application::bootstrap::{BootstrapError, CredentialPreparation, PreparedCredential};
use darkhorse_application::password_change::{
    PasswordReplacementPreparation, PreparedPasswordReplacement,
};
use darkhorse_domain::identity::{CredentialId, PrincipalId};
use std::sync::Arc;
use tokio::sync::Semaphore;
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct PasswordPreparation {
    slots: Arc<Semaphore>,
}

impl Default for PasswordPreparation {
    fn default() -> Self {
        Self {
            slots: Arc::new(Semaphore::new(1)),
        }
    }
}

impl CredentialPreparation for PasswordPreparation {
    async fn prepare(&self, password: &str) -> Result<PreparedCredential, BootstrapError> {
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| BootstrapError::SecretPreparation)?;
        let bytes = random_material()?;
        let password = Zeroizing::new(password.to_owned());
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            prepare_with_material(&password, bytes)
        })
        .await
        .map_err(|_| BootstrapError::SecretPreparation)?
    }
}

impl PasswordReplacementPreparation for PasswordPreparation {
    async fn prepare_replacement(
        &self,
        password: &str,
    ) -> Result<PreparedPasswordReplacement, AuthError> {
        let password = Zeroizing::new(password.to_owned());
        bounded_work(self.slots.clone(), move || {
            let bytes = random_material().map_err(|_| AuthError::Unavailable)?;
            replacement_with_material(&password, bytes)
        })
        .await
    }
}

fn replacement_with_material(
    password: &str,
    bytes: [u8; 32],
) -> Result<PreparedPasswordReplacement, AuthError> {
    let mut credential = [0; 16];
    credential.copy_from_slice(&bytes[..16]);
    Ok(PreparedPasswordReplacement {
        credential_id: credential_id(credential).map_err(|_| AuthError::Unavailable)?,
        verifier: hash(password, &bytes[16..]).map_err(|_| AuthError::Unavailable)?,
    })
}

fn random_material<const N: usize>() -> Result<[u8; N], BootstrapError> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|_| BootstrapError::SecretPreparation)?;
    Ok(bytes)
}

fn prepare_with_material(
    password: &str,
    bytes: [u8; 48],
) -> Result<PreparedCredential, BootstrapError> {
    let mut principal = [0; 16];
    let mut credential = [0; 16];
    principal.copy_from_slice(&bytes[..16]);
    credential.copy_from_slice(&bytes[16..32]);
    let principal_id = PrincipalId::from_u128(
        uuid::Builder::from_random_bytes(principal)
            .into_uuid()
            .as_u128(),
    )
    .map_err(|_| BootstrapError::SecretPreparation)?;
    Ok(PreparedCredential {
        principal_id,
        credential_id: credential_id(credential)?,
        verifier: hash(password, &bytes[32..])?,
    })
}

fn credential_id(bytes: [u8; 16]) -> Result<CredentialId, BootstrapError> {
    CredentialId::from_u128(
        uuid::Builder::from_random_bytes(bytes)
            .into_uuid()
            .as_u128(),
    )
    .map_err(|_| BootstrapError::SecretPreparation)
}

fn hash(password: &str, salt: &[u8]) -> Result<String, BootstrapError> {
    let params =
        Params::new(65536, 3, 1, Some(32)).map_err(|_| BootstrapError::SecretPreparation)?;
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_with_salt(password.as_bytes(), salt)
        .map(|hash| hash.to_string())
        .map_err(|_| BootstrapError::SecretPreparation)
}

// Policy-matched, public dummy. A missing account can never authenticate, even
// if a supplied password happened to match this output.
const DUMMY: &str = "$argon2id$v=19$m=65536,t=3,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

impl PasswordVerification for PasswordPreparation {
    async fn verify(&self, password: &str, verifier: Option<&str>) -> Result<bool, AuthError> {
        darkhorse_domain::authentication::login_password(password)
            .map_err(|_| AuthError::Denied)?;
        let parsed = checked_verifier(verifier.unwrap_or(DUMMY))?;
        let exists = verifier.is_some();
        let password = Zeroizing::new(password.to_owned());
        bounded_work(self.slots.clone(), move || {
            Ok(Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
                && exists)
        })
        .await
    }
}

fn checked_verifier(value: &str) -> Result<PasswordHash, AuthError> {
    if value.len() > 512 || !value.starts_with("$argon2id$v=19$m=65536,t=3,p=1$") {
        return Err(AuthError::Unavailable);
    }
    let parsed = PasswordHash::new(value).map_err(|_| AuthError::Unavailable)?;
    if parsed.salt.as_ref().is_none_or(|salt| salt.len() != 16)
        || parsed.hash.as_ref().is_none_or(|hash| hash.len() != 32)
    {
        return Err(AuthError::Unavailable);
    }
    Ok(parsed)
}

async fn bounded_work<T: Send + 'static>(
    slots: Arc<Semaphore>,
    work: impl FnOnce() -> Result<T, AuthError> + Send + 'static,
) -> Result<T, AuthError> {
    let permit = slots
        .try_acquire_owned()
        .map_err(|_| AuthError::Unavailable)?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work()
    })
    .await
    .map_err(|_| AuthError::Unavailable)?
}

#[cfg(test)]
#[path = "../tests/unit/password.rs"]
mod tests;

impl darkhorse_application::invitations::PasswordPreparation for PasswordPreparation {
    async fn prepare_password(
        &self,
        password: &str,
    ) -> Result<PreparedCredential, darkhorse_domain::invitations::Error> {
        self.prepare(password)
            .await
            .map_err(|_| darkhorse_domain::invitations::Error::Unavailable)
    }
}
