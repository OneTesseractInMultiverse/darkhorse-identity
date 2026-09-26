use super::{
    command::Command,
    output::{Failure, Format},
};
use clap::{CommandFactory, FromArgMatches, error::ErrorKind};
use darkhorse_domain::localization::Locale;
use std::ffi::OsString;
mod help;
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
    pub auth_stdin: bool,
}

pub fn invocation(args: &[OsString]) -> Result<Plan, Failure> {
    invocation_in(args, explicit_locale(args)?.unwrap_or(Locale::English))
}
pub fn invocation_in(args: &[OsString], locale: Locale) -> Result<Plan, Failure> {
    let arguments = validate(args)?;
    let mut command = tree::Options::command();
    command.build();
    match help::localized(command, locale)
        .try_get_matches_from(std::iter::once("darkhorse-server").chain(arguments))
    {
        Ok(matches) => tree::Options::from_arg_matches(&matches)
            .map_err(|_| Failure::usage())
            .and_then(translate::invocation)
            .map(Plan::Run),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            Ok(Plan::Display(help::labels(error.to_string(), locale)))
        }
        Err(_) => Err(Failure::usage()),
    }
}
// Clap performs both passes. This permissive pass selects presentation only;
// the ordinary, strict command tree must still approve every executed command.
pub fn explicit_locale(args: &[OsString]) -> Result<Option<Locale>, Failure> {
    Ok(presentation(args)?.locale)
}
pub struct Presentation {
    pub locale: Option<Locale>,
    pub format: Format,
}
pub fn presentation(args: &[OsString]) -> Result<Presentation, Failure> {
    let arguments = validate(args)?;
    let probe = presentation_tree(tree::Options::command())
        .ignore_errors(true)
        .arg(
            clap::Arg::new("presentation-help")
                .long("help")
                .short('h')
                .global(true)
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            clap::Arg::new("presentation-version")
                .long("version")
                .short('V')
                .global(true)
                .action(clap::ArgAction::SetTrue),
        );
    let matches = probe
        .try_get_matches_from(std::iter::once("darkhorse-server").chain(arguments))
        .map_err(|_| Failure::usage())?;
    let locale = matches
        .get_one::<String>("locale")
        .map(|value| crate::localization::parse(value).map_err(|_| Failure::usage()))
        .transpose()?;
    Ok(Presentation {
        locale,
        format: matches
            .get_one::<Format>("output")
            .copied()
            .unwrap_or_default(),
    })
}
fn presentation_tree(command: clap::Command) -> clap::Command {
    command
        .disable_help_flag(true)
        .disable_version_flag(true)
        .disable_help_subcommand(true)
        .mut_args(|arg| {
            if arg.get_id() == "locale" {
                arg.value_parser(clap::value_parser!(String))
            } else {
                arg
            }
        })
        .mut_subcommands(presentation_tree)
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
