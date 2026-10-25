//! Durable claims are committed before signing or network access.
use super::{LogoutClaims, LogoutSigner};
use crate::signing::WrappedKey;
use darkhorse_domain::{identity::LogoutDeliveryId, logout::DeliveryResult, tokens::Error};
use std::future::Future;

pub struct Delivery {
    pub id: LogoutDeliveryId,
    pub attempt: u16,
    pub destination: String,
    pub key: WrappedKey,
    pub claims: LogoutClaims,
}
pub trait Queue: Send + Sync {
    fn claim(&self) -> impl Future<Output = Result<Option<Delivery>, Error>> + Send;
    fn finish(
        &self,
        id: LogoutDeliveryId,
        attempt: u16,
        result: DeliveryResult,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
pub trait Sender: Send + Sync {
    fn send(&self, destination: &str, token: &str) -> impl Future<Output = DeliveryResult> + Send;
}
pub async fn deliver_next(
    queue: &impl Queue,
    signer: &impl LogoutSigner,
    sender: &impl Sender,
) -> Result<bool, Error> {
    let Some(delivery) = queue.claim().await? else {
        return Ok(false);
    };
    let result = match signer.sign_logout(delivery.key, delivery.claims).await {
        Ok(token) => sender.send(&delivery.destination, &token).await,
        Err(_) => DeliveryResult::Retry,
    };
    queue.finish(delivery.id, delivery.attempt, result).await?;
    Ok(true)
}

#[cfg(test)]
#[path = "../../tests/unit/logout/delivery.rs"]
mod tests;
