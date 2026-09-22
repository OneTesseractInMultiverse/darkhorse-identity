use serde::Deserialize;
use std::io::{self, IsTerminal, Read, Write};
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

pub fn interactive() -> Result<BootstrapInput, &'static str> {
    if !io::stdin().is_terminal() {
        return Err(
            "Interactive bootstrap requires a terminal; use --stdin for protected piped input.",
        );
    }
    let email = prompt("Email: ")?;
    let first_name = prompt("First name: ")?;
    let last_name = prompt("Last name: ")?;
    let password = hidden_password("Password: ")?;
    let confirmation = hidden_password("Confirm password: ")?;
    if *password != *confirmation {
        return Err("Password confirmation does not match.");
    }
    Ok(BootstrapInput {
        email,
        first_name,
        last_name,
        password: password.to_string(),
    })
}

fn hidden_password(prompt: &'static str) -> Result<Zeroizing<String>, &'static str> {
    let config = rpassword::ConfigBuilder::new()
        .output_writer(io::stderr())
        .build();
    let value = Zeroizing::new(
        rpassword::prompt_password_with_config(prompt, config)
            .map_err(|_| "Cannot read password.")?,
    );
    if value.is_empty() || value.len() > 1024 {
        return Err("Invalid password input length.");
    }
    Ok(value)
}

fn prompt(message: &str) -> Result<String, &'static str> {
    use std::io::BufRead;
    io::stderr()
        .write_all(message.as_bytes())
        .and_then(|()| io::stderr().flush())
        .map_err(|_| "Cannot write prompt.")?;
    let mut value = String::new();
    io::stdin()
        .lock()
        .take(1025)
        .read_line(&mut value)
        .map_err(|_| "Cannot read profile input.")?;
    if value.is_empty() {
        return Err("Profile input cancelled.");
    }
    if value.len() > 1024 {
        return Err("Profile input is too long.");
    }
    Ok(value.trim().to_owned())
}

#[cfg(test)]
#[path = "../../tests/unit/operator/input.rs"]
mod tests;
