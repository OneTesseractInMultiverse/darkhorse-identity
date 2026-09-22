use serde::Deserialize;
use std::io::Read;
use zeroize::{Zeroize, Zeroizing};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    pub email: String,
    pub password: String,
    pub reason: Option<String>,
}
impl Drop for Input {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}
pub(super) fn read(reader: impl Read) -> Result<Input, &'static str> {
    let mut bytes = Zeroizing::new(Vec::new());
    reader
        .take(super::super::input::INPUT_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read account authentication input.")?;
    parse(&bytes)
}
fn parse(bytes: &[u8]) -> Result<Input, &'static str> {
    if bytes.len() > super::super::input::INPUT_LIMIT {
        return Err("Account authentication input is too long.");
    }
    serde_json::from_slice(bytes).map_err(|_| "Invalid account authentication input.")
}
pub(super) async fn interactive(mutation: bool) -> Result<Input, &'static str> {
    let mut terminal = super::super::terminal::Terminal::open()?;
    let email = terminal.prompt("Administrator email: ").await?;
    let reason = if mutation {
        Some(terminal.prompt("Reason (no secrets): ").await?.to_string())
    } else {
        None
    };
    terminal.hide()?;
    let password = terminal.prompt("Administrator password: ").await?;
    terminal.restore()?;
    Ok(Input {
        email: email.to_string(),
        password: password.to_string(),
        reason,
    })
}
#[cfg(test)]
#[path = "../../../tests/unit/operator/accounts/input.rs"]
mod tests;
