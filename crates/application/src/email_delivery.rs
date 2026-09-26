//! Transport-independent delivery data. The identifier keeps each proof purpose distinct.
pub struct Delivery<I> {
    pub created_ms: u64,
    pub expires_ms: u64,
    pub locale: darkhorse_domain::localization::Locale,
    pub template_version: u16,
    pub id: I,
    pub attempt: u16,
    pub email: String,
    pub seed: [u8; 32],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryResult {
    Accepted,
    Retry,
    Rejected,
}
