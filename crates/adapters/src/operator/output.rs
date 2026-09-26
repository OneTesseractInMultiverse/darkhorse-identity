use darkhorse_domain::localization::Locale;
use serde::Serialize;
use serde_json::Value;
use std::io::Write;

pub const OUTPUT_LIMIT: usize = 65_536;
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Default)]
pub enum Format {
    #[default]
    Human,
    Json,
}

pub struct Output {
    pub data: Value,
    message: Option<String>,
    spanish: Option<String>,
}
impl Output {
    pub fn record(data: Value) -> Self {
        Self {
            data,
            message: None,
            spanish: None,
        }
    }
    pub fn message(message: impl Into<String>, data: Value) -> Self {
        Self {
            data,
            message: Some(message.into()),
            spanish: None,
        }
    }
    pub fn localized_message(english: String, spanish: String, data: Value) -> Self {
        Self {
            data,
            message: Some(english),
            spanish: Some(spanish),
        }
    }
}
#[derive(Debug)]
pub struct Failure {
    code: &'static str,
    message: &'static str,
    exit: u8,
    pub data: Option<Value>,
}
impl Failure {
    pub fn interrupted() -> Self {
        Self {
            code: "operation_interrupted",
            message: "Operator command interrupted. The operation may have committed; inspect current state before retrying.",
            exit: 1,
            data: None,
        }
    }
    pub fn usage() -> Self {
        Self {
            code: "invalid_arguments",
            message: "Invalid command or arguments; use --help. Secret arguments are not accepted.",
            exit: 2,
            data: None,
        }
    }
    pub fn confirmation() -> Self {
        Self {
            code: "confirmation_required",
            message: "Operation not confirmed. Review the command and supply --yes for noninteractive execution.",
            exit: 3,
            data: None,
        }
    }
    fn output() -> Self {
        Self {
            code: "output_failed",
            message: "Cannot render or write command output. The operation may have committed; inspect current state before retrying.",
            exit: 74,
            data: None,
        }
    }
    pub fn exit_code(&self) -> u8 {
        self.exit
    }
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}
impl From<&'static str> for Failure {
    fn from(message: &'static str) -> Self {
        Self {
            code: "operation_failed",
            message,
            exit: 1,
            data: None,
        }
    }
}
struct Bounded(Vec<u8>);
impl Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > OUTPUT_LIMIT.saturating_sub(self.0.len()) {
            return Err(std::io::ErrorKind::FileTooLarge.into());
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn json(value: &impl Serialize) -> Result<Vec<u8>, Failure> {
    let mut bytes = Bounded(Vec::new());
    serde_json::to_writer(&mut bytes, value).map_err(|_| Failure::output())?;
    let value = String::from_utf8(bytes.0).map_err(|_| Failure::output())?;
    terminal_line(&value)
}
// JSON escaping keeps compatibility with scripts while neutralizing terminal controls and bidi overrides.
fn terminal_line(value: &str) -> Result<Vec<u8>, Failure> {
    let mut bytes = Bounded(Vec::new());
    for character in value.chars() {
        if character.is_control()
            || matches!(character,'\u{2028}'..='\u{202e}'|'\u{2066}'..='\u{2069}'|'\u{061c}'|'\u{200e}'|'\u{200f}')
        {
            write!(&mut bytes, "\\u{:04x}", character as u32).map_err(|_| Failure::output())?;
        } else {
            write!(&mut bytes, "{character}").map_err(|_| Failure::output())?;
        }
    }
    bytes.write_all(b"\n").map_err(|_| Failure::output())?;
    Ok(bytes.0)
}
pub fn render(output: &Output, format: Format) -> Result<Vec<u8>, Failure> {
    render_in(output, format, Locale::English)
}
pub fn render_in(output: &Output, format: Format, locale: Locale) -> Result<Vec<u8>, Failure> {
    match format {
        Format::Json => json(&serde_json::json!({"schema_version":1,"ok":true,"data":output.data})),
        Format::Human => match if locale == Locale::Spanish {
            output.spanish.as_ref().or(output.message.as_ref())
        } else {
            output.message.as_ref()
        } {
            Some(message) => terminal_line(message),
            None if locale == Locale::Spanish => {
                let mut bytes = Bounded(b"Resultado:\n".to_vec());
                bytes
                    .write_all(&json(&output.data)?)
                    .map_err(|_| Failure::output())?;
                Ok(bytes.0)
            }
            None => json(&output.data),
        },
    }
}
pub fn render_failure(error: &Failure, format: Format) -> Result<Vec<u8>, Failure> {
    render_failure_in(error, format, Locale::English)
}
pub fn render_failure_in(
    error: &Failure,
    format: Format,
    locale: Locale,
) -> Result<Vec<u8>, Failure> {
    match format {
        Format::Json => json(
            &serde_json::json!({"schema_version":1,"ok":false,"error":{"code":error.code,"message":error.message},"data":error.data}),
        ),
        Format::Human => terminal_line(super::localization::text(locale, error.message)),
    }
}
pub fn write_bytes(writer: &mut impl Write, bytes: &[u8]) -> Result<(), Failure> {
    writer
        .write_all(bytes)
        .and_then(|()| writer.flush())
        .map_err(|_| Failure::output())
}
pub fn display(text: &str) -> Result<(), Failure> {
    write_bytes(&mut std::io::stdout().lock(), text.as_bytes())
}
pub fn emit(output: &Output, format: Format) -> Result<(), Failure> {
    emit_in(output, format, Locale::English)
}
pub fn emit_in(output: &Output, format: Format, locale: Locale) -> Result<(), Failure> {
    write_bytes(
        &mut std::io::stdout().lock(),
        &render_in(output, format, locale)?,
    )
}
pub fn diagnose(error: &Failure, format: Format) -> Result<(), Failure> {
    diagnose_in(error, format, Locale::English)
}
pub fn diagnose_in(error: &Failure, format: Format, locale: Locale) -> Result<(), Failure> {
    if format == Format::Human
        && let Some(data) = &error.data
    {
        write_bytes(&mut std::io::stdout().lock(), &json(data)?)?;
    }
    write_bytes(
        &mut std::io::stderr().lock(),
        &render_failure_in(error, format, locale)?,
    )
}

#[cfg(test)]
#[path = "../../tests/unit/operator/output.rs"]
mod tests;
