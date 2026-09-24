use super::Format;
use crate::operator::command::{counter, identifier};
use clap::{Args, Parser, Subcommand};
use darkhorse_domain::identity::PrincipalId;

#[derive(Parser)]
#[command(
    name = "darkhorse-server",
    version,
    about = "Darkhorse identity server and deployment-credential operator tools.",
    disable_help_subcommand = false
)]
pub(super) struct Options {
    #[arg(long,global=true,value_enum,default_value_t=Format::Human)]
    pub output: Format,
    /// Confirm a mutation; does not grant authority or bypass safety checks.
    #[arg(long, global = true)]
    pub yes: bool,
    /// Output is always uncolored.
    #[arg(long, global = true)]
    pub no_color: bool,
    /// Read account administrator email/password and mutation reason as protected JSON.
    #[arg(long, global = true)]
    pub auth_stdin: bool,
    #[command(subcommand)]
    pub command: Option<Root>,
}
#[derive(Subcommand)]
pub(super) enum Root {
    /// Run the HTTP server (also the default with no arguments).
    Serve,
    /// Existing local tools using trusted deployment credentials; see docs/operator-authority.md.
    #[command(subcommand)]
    Operator(Operator),
    #[command(flatten)]
    Legacy(Legacy),
}
#[derive(Subcommand)]
pub(super) enum Operator {
    /// Apply embedded database migrations using the schema owner.
    Migrate,
    /// Create the single initial administrator.
    Bootstrap(Bootstrap),
    #[command(subcommand)]
    Account(Account),
    #[command(subcommand)]
    Signing(Signing),
    #[command(subcommand)]
    Limiter(Limiter),
    #[command(subcommand)]
    Redis(Redis),
}
#[derive(Args)]
pub(super) struct Bootstrap {
    /// Read bounded protected JSON from standard input.
    #[arg(long)]
    pub stdin: bool,
}
#[derive(Args)]
pub(super) struct Principal {
    #[arg(value_parser=identifier)]
    pub id: PrincipalId,
}
#[derive(Args)]
pub(super) struct Change {
    #[arg(value_parser=identifier)]
    pub id: PrincipalId,
    #[arg(value_parser=counter)]
    pub revision: u64,
}
#[derive(Args)]
pub(super) struct Revision {
    #[arg(value_parser=counter)]
    pub revision: u64,
}
#[derive(Args)]
pub(super) struct Import {
    #[arg(long, required = true)]
    pub stdin: bool,
    #[arg(value_parser=counter)]
    pub revision: u64,
}
#[derive(Args)]
pub(super) struct Key {
    #[arg(value_parser=crate::operator::signing::identifier, allow_hyphen_values=true)]
    pub kid: [u8; 32],
    #[arg(value_parser=counter)]
    pub revision: u64,
}
#[derive(Subcommand)]
pub(super) enum Account {
    Show(Principal),
    Deactivate(Change),
    Reactivate(Change),
    RevokeAll(Change),
}
#[derive(Subcommand)]
pub(super) enum Signing {
    Status,
    /// Inspect one signing operation using only the primary database.
    Inspect {
        #[arg(value_parser=crate::operator::command::operation_identifier)]
        id: darkhorse_domain::identity::OperationId,
    },
    Generate(Revision),
    Import(Import),
    Activate(Key),
    Retire(Key),
}
#[derive(Subcommand)]
pub(super) enum Limiter {
    Status,
    /// Inspect one activation record without contacting Redis or changing state.
    Inspect {
        #[arg(value_parser=crate::operator::command::operation_identifier)]
        id: darkhorse_domain::identity::OperationId,
    },
    Fence,
    Activate,
}
#[derive(Subcommand)]
pub(super) enum Redis {
    Status,
}
// Hidden compatibility spellings retain the same validation, dispatch and confirmation policy.
#[derive(Subcommand)]
pub(super) enum Legacy {
    #[command(hide = true)]
    Migrate,
    #[command(hide = true)]
    Bootstrap(Bootstrap),
    #[command(hide = true)]
    Account(Principal),
    #[command(hide = true)]
    Deactivate(Change),
    #[command(hide = true)]
    Reactivate(Change),
    #[command(hide = true)]
    RevokeAll(Change),
    #[command(hide = true)]
    SigningStatus,
    #[command(hide = true)]
    SigningGenerate(Revision),
    #[command(hide = true)]
    SigningImport(Import),
    #[command(hide = true)]
    SigningActivate(Key),
    #[command(hide = true)]
    SigningRetire(Key),
    #[command(hide = true)]
    LimiterStatus,
    #[command(hide = true)]
    LimiterFence,
    #[command(hide = true)]
    LimiterActivate,
    #[command(hide = true)]
    RedisStatus,
}
