//! Public lifecycle facts, never secret values or verifiers.
use darkhorse_domain::identity::ClientSecretId;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub id: ClientSecretId,
    pub created_ms: u64,
    pub expires_ms: Option<u64>,
    pub retired: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inventory {
    pub revision: u64,
    pub observed_ms: u64,
    pub items: Vec<Metadata>,
    pub next: Option<ClientSecretId>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Listed(Inventory),
    Retired { revision: u64 },
}
impl Outcome {
    pub fn revision(&self) -> u64 {
        match self {
            Self::Listed(page) => page.revision,
            Self::Retired { revision } => *revision,
        }
    }
}
