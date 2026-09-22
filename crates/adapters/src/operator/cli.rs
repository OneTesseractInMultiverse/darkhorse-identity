use super::{
    command::Command,
    output::{Failure, Format},
};
use clap::{Parser, error::ErrorKind};
use std::ffi::OsString;
mod translate;
mod tree;
pub const ARGUMENT_LIMIT: usize = 32;
#[derive(Debug)]
pub enum Plan {
    Display(String),
    Run(Invocation),
}
#[derive(Debug)]
pub struct Invocation {
    pub command: Command,
    pub format: Format,
    pub confirmed: bool,
}

pub fn invocation(args: &[OsString]) -> Result<Plan, Failure> {
    let arguments = validate(args)?;
    match tree::Options::try_parse_from(std::iter::once("darkhorse-server").chain(arguments)) {
        Ok(options) => translate::invocation(options).map(Plan::Run),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            Ok(Plan::Display(error.to_string()))
        }
        Err(_) => Err(Failure::usage()),
    }
}
fn validate(args: &[OsString]) -> Result<Vec<&str>, Failure> {
    if args.len() > ARGUMENT_LIMIT {
        return Err(Failure::usage());
    }
    let mut total = 0;
    args.iter()
        .map(|arg| {
            let value = arg.to_str().ok_or_else(Failure::usage)?;
            total += value.len();
            if value.len() > 1024 || total > 4096 || value.chars().any(char::is_control) {
                return Err(Failure::usage());
            }
            Ok(value)
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/unit/operator/cli.rs"]
mod tests;
