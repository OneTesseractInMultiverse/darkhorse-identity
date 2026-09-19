//! Invitation workflows. Stores own durable admission and atomic creation/revocation.
use crate::{credentials::PreparedCredential, email_delivery::DeliveryResult};
use darkhorse_domain::{
    directory::{Profile, validate_password},
    identity::{InvitationId, PrincipalId},
    invitations::{self as policy, Error},
};
use std::future::Future;
pub struct Material {
    pub id: InvitationId,
    pub seed: [u8; 32],
    pub digest: [u8; 32],
}
pub trait InvitationSecrets: Send + Sync {
    fn issue_invitation(&self) -> Result<Material, Error>;
}
pub trait PasswordPreparation: Send + Sync {
    fn prepare_password(
        &self,
        password: &str,
    ) -> impl Future<Output = Result<PreparedCredential, Error>> + Send;
}
pub struct Acceptance<'a> {
    pub email: &'a str,
    pub first_name: &'a str,
    pub last_name: &'a str,
    pub password: &'a str,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Record {
    pub id: InvitationId,
    pub email: String,
    pub created_ms: u64,
    pub expires_ms: u64,
    pub closed: bool,
    pub delivery: String,
}
pub trait InvitationStore: Send + Sync {
    fn invitation_preflight(
        &self,
        actor: [u8; 32],
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn invite(
        &self,
        actor: [u8; 32],
        email: &str,
        material: Material,
    ) -> impl Future<Output = Result<InvitationId, Error>> + Send;
    fn invitations(
        &self,
        actor: [u8; 32],
    ) -> impl Future<Output = Result<Vec<Record>, Error>> + Send;
    fn revoke_invitation(
        &self,
        actor: [u8; 32],
        id: InvitationId,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    /// Commits a limited attempt before expensive hashing. Failed hashing does not refund it.
    fn admit_invitation(
        &self,
        digest: [u8; 32],
        email_key: &str,
    ) -> impl Future<Output = Result<InvitationId, Error>> + Send;
    /// Rechecks live proof/issuer/recipient after hashing; creates only an ordinary principal.
    fn accept_invitation(
        &self,
        id: InvitationId,
        digest: [u8; 32],
        profile: Profile,
        credential: PreparedCredential,
    ) -> impl Future<Output = Result<PrincipalId, Error>> + Send;
}
pub async fn invite(
    store: &impl InvitationStore,
    secrets: &impl InvitationSecrets,
    actor: [u8; 32],
    email: &str,
) -> Result<InvitationId, Error> {
    let email = policy::email(email)?;
    store.invitation_preflight(actor).await?;
    let material = secrets.issue_invitation()?;
    store.invite(actor, &email, material).await
}
pub async fn accept(
    store: &impl InvitationStore,
    passwords: &impl PasswordPreparation,
    digest: [u8; 32],
    request: Acceptance<'_>,
) -> Result<PrincipalId, Error> {
    let profile = Profile::new(request.email, request.first_name, request.last_name)
        .map_err(|_| Error::Invalid)?;
    validate_password(request.password).map_err(|_| Error::Invalid)?;
    let id = store.admit_invitation(digest, profile.email_key()).await?;
    let credential = passwords.prepare_password(request.password).await?;
    store
        .accept_invitation(id, digest, profile, credential)
        .await
}
pub type Delivery = crate::email_delivery::Delivery<InvitationId>;
pub trait InvitationDelivery: Send + Sync {
    fn deliver_invitation(
        &self,
        delivery: &Delivery,
    ) -> impl Future<Output = DeliveryResult> + Send;
}
pub trait InvitationQueue: Send + Sync {
    fn claim_invitation(&self) -> impl Future<Output = Result<Option<Delivery>, Error>> + Send;
    fn finish_invitation(
        &self,
        id: InvitationId,
        attempt: u16,
        result: DeliveryResult,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
pub async fn deliver_next(
    queue: &impl InvitationQueue,
    sender: &impl InvitationDelivery,
) -> Result<bool, Error> {
    let Some(delivery) = queue.claim_invitation().await? else {
        return Ok(false);
    };
    let result = sender.deliver_invitation(&delivery).await;
    queue
        .finish_invitation(delivery.id, delivery.attempt, result)
        .await?;
    Ok(true)
}
#[cfg(test)]
#[path = "../tests/unit/invitations.rs"]
mod tests;
