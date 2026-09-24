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
    /// Read administrator email/password and an optional mutation reason as protected JSON.
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
    /// Apply embedded database migrations, or inspect an earlier operation, using the database owner.
    Migrate(Migration),
    /// Create the single initial administrator.
    Bootstrap(Bootstrap),
    #[command(subcommand)]
    Account(Account),
    #[command(subcommand)]
    Application(Application),
    #[command(subcommand)]
    Client(Client),
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
    /// Read one authenticated page; repeat explicitly with the returned cursor.
    List(List),
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

#[derive(Args)]
pub(super) struct Migration {
    #[command(subcommand)]
    pub command: Option<MigrationCommand>,
}
#[derive(Subcommand)]
pub(super) enum MigrationCommand {
    /// Read recorded progress and current history without running migrations.
    Inspect {
        #[arg(value_parser=crate::operator::command::operation_identifier)]
        id: darkhorse_domain::identity::OperationId,
    },
}

#[derive(Args)]
pub(super) struct List {
    #[arg(long, default_value = "")]
    pub search: String,
    #[arg(long, value_enum)]
    pub status: Option<Status>,
    #[arg(long, value_parser=identifier)]
    pub after: Option<PrincipalId>,
    #[arg(long, default_value_t=25, value_parser=clap::value_parser!(u16).range(1..=25))]
    pub limit: u16,
}
#[derive(Clone, Copy, clap::ValueEnum)]
pub(super) enum Status {
    Active,
    Inactive,
}

#[derive(Subcommand)]
pub(super) enum Application {
    /// Read one application's configuration after fresh administrator authentication.
    Show {
        #[arg(value_parser=crate::operator::command::application_identifier)]
        application: darkhorse_domain::identity::ApplicationId,
    },
    /// Read one authenticated page of applications.
    List(CatalogList),
}
#[derive(Subcommand)]
pub(super) enum Client {
    /// Read one client's configuration within its application; excludes credentials.
    Show {
        #[arg(value_parser=crate::operator::command::application_identifier)]
        application: darkhorse_domain::identity::ApplicationId,
        #[arg(value_parser=crate::operator::command::client_identifier)]
        client: darkhorse_domain::identity::ClientId,
    },
    /// Read one authenticated page of clients belonging to an application.
    List {
        #[arg(value_parser=crate::operator::command::application_identifier)]
        application: darkhorse_domain::identity::ApplicationId,
        #[command(flatten)]
        query: CatalogList,
    },
}
#[derive(Args)]
pub(super) struct CatalogList {
    #[arg(long, default_value = "")]
    pub search: String,
    #[arg(long, value_enum)]
    pub status: Option<Status>,
    #[arg(long, value_parser=catalog_cursor)]
    pub after: Option<std::num::NonZeroU128>,
    #[arg(long, default_value_t=25, value_parser=clap::value_parser!(u16).range(1..=25))]
    pub limit: u16,
}

fn catalog_cursor(value: &str) -> Result<std::num::NonZeroU128, &'static str> {
    uuid::Uuid::parse_str(value)
        .ok()
        .and_then(|v| std::num::NonZeroU128::new(v.as_u128()))
        .ok_or("Invalid catalog continuation.")
}
