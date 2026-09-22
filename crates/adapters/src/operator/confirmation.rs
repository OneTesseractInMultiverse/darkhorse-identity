use super::{
    command::{Command, requires_confirmation},
    output::{Failure, Format, write_bytes},
};
use std::io::{BufRead, IsTerminal, Read};
#[derive(Debug, PartialEq, Eq)]
enum Decision {
    Proceed,
    Prompt,
    Deny,
}
fn decision(command: Command, confirmed: bool, terminal: bool, format: Format) -> Decision {
    if !requires_confirmation(command) || confirmed {
        return Decision::Proceed;
    }
    if !terminal || consumes_stdin(command) || format == Format::Json {
        return Decision::Deny;
    }
    Decision::Prompt
}
pub fn confirm(command: Command, confirmed: bool, format: Format) -> Result<(), Failure> {
    match decision(command, confirmed, std::io::stdin().is_terminal(), format) {
        Decision::Proceed => Ok(()),
        Decision::Deny => Err(Failure::confirmation()),
        Decision::Prompt => prompt(),
    }
}
fn prompt() -> Result<(), Failure> {
    write_bytes(
        &mut std::io::stderr().lock(),
        b"Confirm this operator mutation by typing yes: ",
    )?;
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .take(5)
        .read_line(&mut line)
        .map_err(|_| Failure::confirmation())?;
    accept(&line)
}
fn consumes_stdin(command: Command) -> bool {
    matches!(
        command,
        Command::Bootstrap { stdin: true } | Command::Signing(super::signing::Operation::Import(_))
    )
}
fn accept(line: &str) -> Result<(), Failure> {
    if matches!(line, "yes\n" | "yes\r\n") {
        Ok(())
    } else {
        Err(Failure::confirmation())
    }
}
#[cfg(test)]
#[path = "../../tests/unit/operator/confirmation.rs"]
mod tests;
