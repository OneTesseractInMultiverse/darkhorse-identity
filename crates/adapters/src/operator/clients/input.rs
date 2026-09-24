use super::super::authenticated;
use crate::registration::ClientInput;
use serde::Deserialize;
use std::io::Read;
use zeroize::Zeroizing;

pub(super) const INPUT_LIMIT: usize = 32 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    pub authentication: authenticated::Input,
    pub client: ClientInput,
}
pub(super) fn read(reader: impl Read) -> Result<Input, &'static str> {
    let mut bytes = Zeroizing::new(Vec::new());
    reader
        .take(INPUT_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read client configuration input.")?;
    parse(&bytes)
}
fn parse(bytes: &[u8]) -> Result<Input, &'static str> {
    if bytes.len() > INPUT_LIMIT {
        return Err("Client configuration input is too long.");
    }
    serde_json::from_slice(bytes).map_err(|_| "Invalid client configuration input.")
}
#[cfg(test)]
#[path = "../../../tests/unit/operator/clients/input.rs"]
mod tests;
