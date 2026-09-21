//! Small cache of immutable, integrity-checked bytes. Authority is checked before lookup.
use darkhorse_application::media::Asset;
use darkhorse_domain::identity::AssetId;
use std::collections::VecDeque;
const BYTE_LIMIT: usize = 8 * 1024 * 1024;
const ENTRY_LIMIT: usize = 32;
#[derive(Default)]
pub(super) struct Cache {
    entries: VecDeque<(Asset, Vec<u8>)>,
    bytes: usize,
}
impl Cache {
    pub fn get(&self, asset: &Asset) -> Option<Vec<u8>> {
        self.entries
            .iter()
            .find(|(key, _)| {
                key.id == asset.id && key.digest == asset.digest && key.bytes == asset.bytes
            })
            .map(|(_, bytes)| bytes.clone())
    }
    pub fn remove(&mut self, id: AssetId) {
        if let Some(index) = self.entries.iter().position(|(key, _)| key.id == id)
            && let Some((_, bytes)) = self.entries.remove(index)
        {
            self.bytes -= bytes.len();
        }
    }
    pub fn insert(&mut self, asset: Asset, bytes: Vec<u8>) {
        self.remove(asset.id);
        if bytes.len() > BYTE_LIMIT {
            return;
        }
        while self.bytes + bytes.len() > BYTE_LIMIT || self.entries.len() >= ENTRY_LIMIT {
            if let Some((_, bytes)) = self.entries.pop_front() {
                self.bytes -= bytes.len();
            }
        }
        self.bytes += bytes.len();
        self.entries.push_back((asset, bytes));
    }
}
#[cfg(test)]
#[path = "../../tests/unit/media/cache.rs"]
mod tests;
