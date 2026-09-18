//! Operator-only lifecycle and public key projections; private material has no Debug.
use darkhorse_domain::signing::{KeyError, KeyState};
use std::future::Future;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKey {
    pub kid: String,
    pub n: String,
    pub e: String,
}
pub struct WrappedKey {
    pub public: PublicKey,
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}
#[derive(Debug, Clone)]
pub struct KeyRecord {
    pub public: PublicKey,
    pub state: KeyState,
}
#[derive(Debug)]
pub struct KeyInventory {
    pub revision: u64,
    pub keys: Vec<KeyRecord>,
}
pub trait SigningStore: Send + Sync {
    fn bind_provider(
        &self,
        issuer: &str,
        wrap_digest: [u8; 32],
    ) -> impl Future<Output = Result<(), KeyError>> + Send;
    fn inventory(
        &self,
        issuer: &str,
    ) -> impl Future<Output = Result<KeyInventory, KeyError>> + Send;
    fn stage(
        &self,
        issuer: &str,
        wrap_digest: [u8; 32],
        expected: u64,
        key: WrappedKey,
    ) -> impl Future<Output = Result<u64, KeyError>> + Send;
    fn activate(
        &self,
        issuer: &str,
        kid: &str,
        expected: u64,
    ) -> impl Future<Output = Result<u64, KeyError>> + Send;
    fn retire(
        &self,
        issuer: &str,
        kid: &str,
        expected: u64,
    ) -> impl Future<Output = Result<u64, KeyError>> + Send;
    fn published(
        &self,
        issuer: &str,
    ) -> impl Future<Output = Result<Vec<PublicKey>, KeyError>> + Send;
}
