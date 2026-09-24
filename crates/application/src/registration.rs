//! Registration use cases. The store rechecks authority and commits audit atomically.
use darkhorse_domain::{identity::*, registration::*};
use std::{future::Future, num::NonZeroU128};

#[derive(Debug, Clone)]
pub enum Command {
    CreateApplication(ApplicationSpec),
    UpdateApplication {
        application: ApplicationId,
        revision: u64,
        spec: ApplicationSpec,
    },
    CreateResource {
        application: ApplicationId,
        name: Label,
    },
    CreateScope {
        application: ApplicationId,
        resource: ResourceId,
        name: ScopeName,
    },
    CreateClient {
        application: ApplicationId,
        spec: ClientSpec,
    },
    UpdateClient {
        application: ApplicationId,
        client: ClientId,
        revision: u64,
        spec: ClientSpec,
    },
    RotateSecret {
        application: ApplicationId,
        client: ClientId,
        revision: u64,
        overlap_seconds: u16,
    },
    RetireSecret {
        application: ApplicationId,
        client: ClientId,
        secret: ClientSecretId,
        revision: u64,
    },
}
impl Command {
    pub fn needs_identifier(&self) -> bool {
        matches!(
            self,
            Self::CreateApplication(_)
                | Self::CreateResource { .. }
                | Self::CreateScope { .. }
                | Self::CreateClient { .. }
        )
    }
    pub fn needs_secret(&self) -> bool {
        matches!(self, Self::CreateClient { .. } | Self::RotateSecret { .. })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadTarget {
    Application(ApplicationId),
    Client {
        application: ApplicationId,
        client: ClientId,
    },
}
#[derive(Debug, Clone)]
pub struct ApplicationRecord {
    pub id: ApplicationId,
    pub name: String,
    pub owner: PrincipalId,
    pub owner_email: String,
    pub active: bool,
    pub revision: u64,
}
#[derive(Debug, Clone)]
pub struct ResourceRecord {
    pub id: ResourceId,
    pub application: ApplicationId,
    pub name: String,
    pub audience: String,
}
#[derive(Debug, Clone)]
pub struct ScopeRecord {
    pub id: ScopeId,
    pub application: ApplicationId,
    pub resource: ResourceId,
    pub name: String,
}
#[derive(Debug, Clone)]
pub struct SecretMetadata {
    pub id: ClientSecretId,
    pub created_ms: u64,
    pub expires_ms: Option<u64>,
}
#[derive(Debug, Clone)]
pub struct ClientRecord {
    pub id: ClientId,
    pub application: ApplicationId,
    pub revision: u64,
    pub spec: ClientSpec,
    pub secrets: Vec<SecretMetadata>,
}
#[derive(Debug, Clone)]
pub enum Record {
    Application(ApplicationRecord),
    Resource(ResourceRecord),
    Scope(ScopeRecord),
    Client(ClientRecord),
}
// Bearers and verifiers deliberately have no Debug implementation.
pub struct SecretVerifier {
    pub id: ClientSecretId,
    pub digest: [u8; 32],
}
pub struct NewSecret {
    pub value: String,
    pub verifier: SecretVerifier,
}
pub struct Prepared {
    pub identifier: Option<NonZeroU128>,
    pub secret: Option<SecretVerifier>,
}
pub struct Written {
    pub record: Record,
    pub secret: Option<String>,
}

pub trait Entropy: Send + Sync {
    fn identifier(&self) -> Result<NonZeroU128, RegistrationError>;
    fn secret(&self) -> Result<NewSecret, RegistrationError>;
}
pub trait RegistrationStore: Send + Sync {
    /// Validate actor, target, revision, owner, grants, and overlap before entropy.
    fn preflight(
        &self,
        actor: [u8; 32],
        command: &Command,
    ) -> impl Future<Output = Result<(), RegistrationError>> + Send;
    /// Revalidate everything under locks. Store only verifiers; audit in the same transaction.
    fn execute(
        &self,
        actor: [u8; 32],
        command: &Command,
        prepared: Prepared,
    ) -> impl Future<Output = Result<Record, RegistrationError>> + Send;
    fn read(
        &self,
        actor: [u8; 32],
        target: ReadTarget,
    ) -> impl Future<Output = Result<Record, RegistrationError>> + Send;
}
/// Current primary-state authentication for the future token transport. No decision cache.
pub trait ClientAuthenticationStore {
    fn authenticate_client(
        &self,
        client: ClientId,
        verifier: [u8; 32],
    ) -> impl Future<Output = Result<ClientRecord, RegistrationError>> + Send;
}
pub trait Registration: Send + Sync {
    fn write(
        &self,
        actor: [u8; 32],
        command: Command,
    ) -> impl Future<Output = Result<Written, RegistrationError>> + Send;
    fn read(
        &self,
        actor: [u8; 32],
        target: ReadTarget,
    ) -> impl Future<Output = Result<Record, RegistrationError>> + Send;
}
pub struct Service<S, E> {
    pub store: S,
    pub entropy: E,
}
impl<S: RegistrationStore, E: Entropy> Registration for Service<S, E> {
    async fn write(&self, actor: [u8; 32], command: Command) -> Result<Written, RegistrationError> {
        self.store.preflight(actor, &command).await?;
        let identifier = command
            .needs_identifier()
            .then(|| self.entropy.identifier())
            .transpose()?;
        let secret = command
            .needs_secret()
            .then(|| self.entropy.secret())
            .transpose()?;
        let (value, verifier) = separate(secret);
        let record = self
            .store
            .execute(
                actor,
                &command,
                Prepared {
                    identifier,
                    secret: verifier,
                },
            )
            .await?;
        Ok(Written {
            record,
            secret: value,
        })
    }
    async fn read(&self, actor: [u8; 32], target: ReadTarget) -> Result<Record, RegistrationError> {
        self.store.read(actor, target).await
    }
}
fn separate(secret: Option<NewSecret>) -> (Option<String>, Option<SecretVerifier>) {
    match secret {
        Some(s) => (Some(s.value), Some(s.verifier)),
        None => (None, None),
    }
}
#[cfg(test)]
#[path = "../tests/unit/registration.rs"]
mod tests;
