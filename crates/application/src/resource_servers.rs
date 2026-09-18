//! Resource-bound introspection credentials and their administrative lifecycle.
use crate::tokens::ActiveToken;
use darkhorse_domain::{
    authorization::CapabilitySet, identity::*, registration::RegistrationError, tokens::Error,
};
use std::future::Future;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub application: ApplicationId,
    pub resource: ResourceId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Register,
    Rotate { revision: u64, overlap_seconds: u16 },
    SetActive { revision: u64, active: bool },
}
impl Change {
    pub fn needs_secret(self) -> bool {
        matches!(self, Self::Register | Self::Rotate { .. })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    pub target: Target,
    pub change: Change,
}
#[derive(Debug, Clone)]
pub struct SecretMetadata {
    pub id: CredentialId,
    pub created_ms: u64,
    pub expires_ms: Option<u64>,
}
#[derive(Debug, Clone)]
pub struct Record {
    pub target: Target,
    pub revision: u64,
    pub active: bool,
    pub secrets: Vec<SecretMetadata>,
}
pub struct Verifier {
    pub id: CredentialId,
    pub digest: [u8; 32],
}
pub struct NewSecret {
    pub value: String,
    pub verifier: Verifier,
}
pub struct Written {
    pub record: Record,
    pub secret: Option<String>,
}
pub trait Entropy: Send + Sync {
    fn secret(&self) -> Result<NewSecret, RegistrationError>;
}
pub trait Store: Send + Sync {
    fn preflight(
        &self,
        actor: [u8; 32],
        command: Command,
    ) -> impl Future<Output = Result<(), RegistrationError>> + Send;
    fn execute(
        &self,
        actor: [u8; 32],
        command: Command,
        secret: Option<Verifier>,
    ) -> impl Future<Output = Result<Record, RegistrationError>> + Send;
    fn read(
        &self,
        actor: [u8; 32],
        target: Target,
    ) -> impl Future<Output = Result<Record, RegistrationError>> + Send;
}
pub trait Registry: Send + Sync {
    fn write(
        &self,
        actor: [u8; 32],
        command: Command,
    ) -> impl Future<Output = Result<Written, RegistrationError>> + Send;
    fn read(
        &self,
        actor: [u8; 32],
        target: Target,
    ) -> impl Future<Output = Result<Record, RegistrationError>> + Send;
}
pub struct Service<S, E> {
    pub store: S,
    pub entropy: E,
}
impl<S: Store, E: Entropy> Registry for Service<S, E> {
    async fn write(&self, actor: [u8; 32], command: Command) -> Result<Written, RegistrationError> {
        self.store.preflight(actor, command).await?;
        let prepared = command
            .change
            .needs_secret()
            .then(|| self.entropy.secret())
            .transpose()?;
        let (secret, verifier) = separate(prepared);
        let record = self.store.execute(actor, command, verifier).await?;
        Ok(Written { record, secret })
    }
    async fn read(&self, actor: [u8; 32], target: Target) -> Result<Record, RegistrationError> {
        self.store.read(actor, target).await
    }
}
fn separate(secret: Option<NewSecret>) -> (Option<String>, Option<Verifier>) {
    match secret {
        Some(secret) => (Some(secret.value), Some(secret.verifier)),
        None => (None, None),
    }
}
// Secrets, verifiers and claim-bearing responses intentionally omit Debug.
pub struct Probe {
    pub resource: ResourceId,
    pub secret: [u8; 32],
    pub token: Option<[u8; 32]>,
}
pub struct ActiveResourceToken {
    pub token: ActiveToken,
    pub resource: ResourceId,
    pub capabilities: CapabilitySet,
}
pub trait ResourceTokenStore: Send + Sync {
    fn introspect_resource(
        &self,
        input: Probe,
        issuer: &str,
    ) -> impl Future<Output = Result<Option<ActiveResourceToken>, Error>> + Send;
}
#[cfg(test)]
#[path = "../tests/unit/resource_servers.rs"]
mod tests;
