use serde::Deserialize;
use std::io::Read;
use zeroize::{Zeroize, Zeroizing};

pub const INPUT_LIMIT: usize = 16384;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapInput {
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub password: String,
}
impl Drop for BootstrapInput {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

pub fn parse(bytes: &[u8]) -> Result<BootstrapInput, &'static str> {
    if bytes.len() > INPUT_LIMIT {
        return Err("Bootstrap input is too long.");
    }
    serde_json::from_slice(bytes).map_err(|_| "Invalid bootstrap input.")
}

pub fn read_json(reader: impl Read) -> Result<BootstrapInput, &'static str> {
    let mut bytes = Zeroizing::new(Vec::new());
    reader
        .take(INPUT_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read bootstrap input.")?;
    parse(&bytes)
}

pub async fn interactive() -> Result<BootstrapInput, &'static str> {
    let mut terminal = super::terminal::Terminal::open()?;
    let email = terminal.prompt("Email: ").await?;
    let first_name = terminal.prompt("First name: ").await?;
    let last_name = terminal.prompt("Last name: ").await?;
    terminal.hide()?;
    let password = terminal.prompt("Password: ").await?;
    let confirmation = terminal.prompt("\nConfirm password: ").await?;
    terminal.restore()?;
    assemble(&email, &first_name, &last_name, &password, &confirmation)
}

fn assemble(
    email: &str,
    first_name: &str,
    last_name: &str,
    password: &str,
    confirmation: &str,
) -> Result<BootstrapInput, &'static str> {
    if password != confirmation {
        return Err("Password confirmation does not match.");
    }
    Ok(BootstrapInput {
        email: email.trim().to_owned(),
        first_name: first_name.trim().to_owned(),
        last_name: last_name.trim().to_owned(),
        password: password.to_string(),
    })
}

#[cfg(test)]
#[path = "../../tests/unit/operator/input.rs"]
mod tests;
