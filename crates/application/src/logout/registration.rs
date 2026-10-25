use darkhorse_domain::{
    identity::{ApplicationId, ClientId, LogoutDeliveryId},
    registration::RegistrationError,
};
use std::future::Future;

#[derive(Clone, Copy)]
pub struct Target {
    pub application: ApplicationId,
    pub client: ClientId,
}
pub struct Configuration {
    pub revision: u64,
    pub uri: Option<String>,
}
pub struct DeliveryStatus {
    pub id: LogoutDeliveryId,
    pub state: String,
    pub attempts: u16,
    pub created_ms: u64,
}
pub struct Record {
    pub configuration: Configuration,
    pub deliveries: Vec<DeliveryStatus>,
}
pub trait Registration: Send + Sync {
    fn logout_configuration(
        &self,
        actor: [u8; 32],
        target: Target,
    ) -> impl Future<Output = Result<Record, RegistrationError>> + Send;
    fn configure_logout(
        &self,
        actor: [u8; 32],
        target: Target,
        configuration: Configuration,
    ) -> impl Future<Output = Result<Configuration, RegistrationError>> + Send;
}
