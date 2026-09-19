//! Atomic email proof and bounded delivery boundaries. No transport or ambient inputs.
use darkhorse_domain::{email_verification::Error, identity::EmailVerificationId};
use std::future::Future;
#[derive(Debug, PartialEq, Eq)]
pub struct Status {
    pub email: String,
    pub verified: bool,
}
pub struct Material {
    pub id: EmailVerificationId,
    pub seed: [u8; 32],
    pub digest: [u8; 32],
}
pub trait VerificationSecrets: Send + Sync {
    fn issue(&self) -> Result<Material, Error>;
}
pub trait VerificationStore: Send + Sync {
    fn email_status(&self, actor: [u8; 32]) -> impl Future<Output = Result<Status, Error>> + Send;
    fn request_verification(
        &self,
        actor: [u8; 32],
        material: Material,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn verify_email(
        &self,
        actor: [u8; 32],
        digest: [u8; 32],
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
pub async fn request(
    store: &impl VerificationStore,
    secrets: &impl VerificationSecrets,
    actor: [u8; 32],
) -> Result<(), Error> {
    store.email_status(actor).await?;
    let material = secrets.issue()?;
    store.request_verification(actor, material).await
}
pub type Delivery = crate::email_delivery::Delivery<EmailVerificationId>;
pub use crate::email_delivery::DeliveryResult;
pub trait EmailDelivery: Send + Sync {
    fn deliver(&self, delivery: &Delivery) -> impl Future<Output = DeliveryResult> + Send;
}
pub trait DeliveryQueue: Send + Sync {
    /// Claim at most one current, unexpired job with a recoverable lease.
    fn claim_email(&self) -> impl Future<Output = Result<Option<Delivery>, Error>> + Send;
    /// A stale acknowledgement must never modify a newer claim or request.
    fn finish_email(
        &self,
        id: EmailVerificationId,
        attempt: u16,
        result: DeliveryResult,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
pub async fn deliver_next(
    queue: &impl DeliveryQueue,
    sender: &impl EmailDelivery,
) -> Result<bool, Error> {
    let Some(delivery) = queue.claim_email().await? else {
        return Ok(false);
    };
    let result = sender.deliver(&delivery).await;
    queue
        .finish_email(delivery.id, delivery.attempt, result)
        .await?;
    Ok(true)
}
#[cfg(test)]
#[path = "../tests/unit/email_verification.rs"]
mod tests;
