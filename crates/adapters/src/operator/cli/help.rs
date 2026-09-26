use crate::operator::localization::messages;
use darkhorse_domain::localization::Locale;

fn translated(value: &clap::builder::StyledStr) -> Option<&'static str> {
    let value = value.to_string();
    messages::spanish(&value).or_else(|| messages::spanish(&format!("{value}.")))
}

// Clap's structural value labels are not configurable. Only static help/version
// output reaches this function; parsing diagnostics and supplied values do not.
pub(super) fn labels(help: String, locale: Locale) -> String {
    if locale == Locale::Spanish {
        help.replace("[possible values:", "[valores posibles:")
            .replace("[default:", "[predeterminado:")
    } else {
        help
    }
}

pub(super) fn localized(command: clap::Command, locale: Locale) -> clap::Command {
    if locale == Locale::English {
        return command;
    }
    let mut command = command;
    if let Some(translated) = command.get_about().and_then(translated) {
        command = command.about(translated).long_about(translated);
    }
    command
        .help_template("{before-help}{about-with-newline}\nUso: {usage}\n\n{all-args}{after-help}")
        .subcommand_help_heading("Comandos")
        .mut_args(|arg| {
            let heading = if arg.is_positional() {
                "Argumentos"
            } else {
                "Opciones"
            };
            let translated = arg.get_help().and_then(translated);
            let arg = arg.help_heading(heading);
            match translated {
                Some(help) => arg.help(help).long_help(help),
                None => arg,
            }
        })
        .mut_subcommands(|command| localized(command, locale))
}
