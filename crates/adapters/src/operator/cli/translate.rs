use super::{Command, Failure, Format, Invocation, tree::*};
use crate::operator::signing::Operation;
use darkhorse_domain::{AccountStatus, directory::AccountAction};
pub(super) fn invocation(options: Options) -> Result<Invocation, Failure> {
    let command = match options.command {
        None | Some(Root::Serve) => Command::Serve,
        Some(Root::Operator(value)) => operator(value)?,
        Some(Root::Legacy(value)) => legacy(value)?,
    };
    if command == Command::Serve && (options.output != Format::Human || options.yes) {
        return Err(Failure::usage());
    }
    if options.output == Format::Json && matches!(command, Command::Bootstrap { stdin: false }) {
        return Err(Failure::usage());
    }
    let account = matches!(
        command,
        Command::Account(_)
            | Command::Accounts(_)
            | Command::Catalog(_)
            | Command::CatalogShow(_)
            | Command::ApplicationMutation(_)
            | Command::ClientUpdate { .. }
            | Command::ClientSecret { .. }
            | Command::Change { .. }
    );
    if (matches!(command, Command::ClientUpdate { .. }) && !options.auth_stdin)
        || (options.auth_stdin && !account)
        || (account && options.output == Format::Json && !options.auth_stdin)
    {
        return Err(Failure::usage());
    }
    Ok(Invocation {
        command,
        format: options.output,
        confirmed: options.yes,
        auth_stdin: options.auth_stdin,
    })
}
fn operator(value: Operator) -> Result<Command, Failure> {
    Ok(match value {
        Operator::Resource(ApplicationCatalog::List { application, query }) => catalog(
            darkhorse_domain::operator_catalog::Target::Resources(application),
            query,
        )?,
        Operator::Scope(ApplicationCatalog::List { application, query }) => catalog(
            darkhorse_domain::operator_catalog::Target::Scopes(application),
            query,
        )?,
        Operator::Role(DefinitionCatalog::List { selection, query }) => catalog(
            darkhorse_domain::operator_catalog::Target::Roles(definitions(selection)),
            query,
        )?,
        Operator::Capability(DefinitionCatalog::List { selection, query }) => catalog(
            darkhorse_domain::operator_catalog::Target::Capabilities(definitions(selection)),
            query,
        )?,
        Operator::Client(Client::Secret(value)) => secret(value),
        Operator::Client(Client::Update {
            application,
            client,
            revision,
        }) => Command::ClientUpdate {
            application,
            client,
            revision,
        },
        Operator::Migrate(Migration { command: None }) => Command::Migrate,
        Operator::Migrate(Migration {
            command: Some(MigrationCommand::Inspect { id }),
        }) => Command::MigrationInspect(id),
        Operator::Bootstrap(value) => Command::Bootstrap { stdin: value.stdin },
        Operator::Account(value) => account(value)?,
        Operator::Application(Application::Create(spec)) => Command::ApplicationMutation(
            darkhorse_domain::operator_applications::Operation::Create(application_spec(spec)),
        ),
        Operator::Application(Application::Update {
            application,
            revision,
            spec,
        }) => Command::ApplicationMutation(
            darkhorse_domain::operator_applications::Operation::Update {
                application,
                revision,
                spec: application_spec(spec),
            },
        ),
        Operator::Application(Application::Show { application }) => Command::CatalogShow(
            darkhorse_application::registration::ReadTarget::Application(application),
        ),
        Operator::Client(Client::Show {
            application,
            client,
        }) => Command::CatalogShow(darkhorse_application::registration::ReadTarget::Client {
            application,
            client,
        }),
        Operator::Application(Application::List(query)) => catalog(
            darkhorse_domain::operator_catalog::Target::Applications,
            query,
        )?,
        Operator::Client(Client::List { application, query }) => catalog(
            darkhorse_domain::operator_catalog::Target::Clients(application),
            query,
        )?,
        Operator::Signing(value) => Command::Signing(signing(value)),
        Operator::Limiter(Limiter::Status) => Command::LimiterStatus,
        Operator::Limiter(Limiter::Inspect { id }) => Command::LimiterInspect(id),
        Operator::Limiter(Limiter::Fence) => Command::LimiterFence,
        Operator::Limiter(Limiter::Activate) => Command::LimiterActivate,
        Operator::Redis(Redis::Status) => Command::RedisStatus,
    })
}
fn application_spec(spec: ApplicationSpec) -> darkhorse_domain::registration::ApplicationSpec {
    darkhorse_domain::registration::ApplicationSpec {
        name: spec.name,
        owner: spec.owner,
        active: matches!(spec.status, Status::Active),
    }
}
fn account(value: Account) -> Result<Command, Failure> {
    Ok(match value {
        Account::List(value) => Command::Accounts(list(value)?),
        Account::Show(value) => Command::Account(value.id),
        Account::Deactivate(value) => {
            change(value, AccountAction::SetStatus(AccountStatus::Inactive))
        }
        Account::Reactivate(value) => {
            change(value, AccountAction::SetStatus(AccountStatus::Active))
        }
        Account::RevokeAll(value) => change(value, AccountAction::RevokeAll),
    })
}
fn change(value: Change, action: AccountAction) -> Command {
    Command::Change {
        id: value.id,
        revision: value.revision,
        action,
    }
}
fn signing(value: Signing) -> Operation {
    match value {
        Signing::Status => Operation::Status,
        Signing::Inspect { id } => Operation::Inspect(id),
        Signing::Generate(value) => Operation::Generate(value.revision),
        Signing::Import(value) => Operation::Import(value.revision),
        Signing::Activate(value) => Operation::Activate {
            kid: value.kid,
            revision: value.revision,
        },
        Signing::Retire(value) => Operation::Retire {
            kid: value.kid,
            revision: value.revision,
        },
    }
}
fn legacy(value: Legacy) -> Result<Command, Failure> {
    operator(match value {
        Legacy::Migrate => Operator::Migrate(Migration { command: None }),
        Legacy::Bootstrap(v) => Operator::Bootstrap(v),
        Legacy::Account(v) => Operator::Account(Account::Show(v)),
        Legacy::Deactivate(v) => Operator::Account(Account::Deactivate(v)),
        Legacy::Reactivate(v) => Operator::Account(Account::Reactivate(v)),
        Legacy::RevokeAll(v) => Operator::Account(Account::RevokeAll(v)),
        Legacy::SigningStatus => Operator::Signing(Signing::Status),
        Legacy::SigningGenerate(v) => Operator::Signing(Signing::Generate(v)),
        Legacy::SigningImport(v) => Operator::Signing(Signing::Import(v)),
        Legacy::SigningActivate(v) => Operator::Signing(Signing::Activate(v)),
        Legacy::SigningRetire(v) => Operator::Signing(Signing::Retire(v)),
        Legacy::LimiterStatus => Operator::Limiter(Limiter::Status),
        Legacy::LimiterFence => Operator::Limiter(Limiter::Fence),
        Legacy::LimiterActivate => Operator::Limiter(Limiter::Activate),
        Legacy::RedisStatus => Operator::Redis(Redis::Status),
    })
}

fn list(value: List) -> Result<darkhorse_domain::operator_directory::Request, Failure> {
    darkhorse_domain::operator_directory::Request::new(darkhorse_domain::admin_directory::Query {
        search: value.search,
        status: value.status.map(|s| match s {
            Status::Active => AccountStatus::Active,
            Status::Inactive => AccountStatus::Inactive,
        }),
        after: value.after,
        limit: value.limit,
    })
    .map_err(|_| Failure::usage())
}

fn catalog(
    target: darkhorse_domain::operator_catalog::Target,
    value: CatalogList,
) -> Result<Command, Failure> {
    darkhorse_domain::operator_catalog::Request::new(
        target,
        darkhorse_domain::admin_catalog::Query {
            search: value.search,
            active: value.status.map(|s| matches!(s, Status::Active)),
            after: value.after,
            limit: value.limit,
        },
    )
    .map(Command::Catalog)
    .map_err(|_| Failure::usage())
}

fn secret(value: ClientSecret) -> Command {
    use darkhorse_domain::operator_client_secrets::{Operation, Target};
    let (target, operation) = match value {
        ClientSecret::List {
            target,
            after,
            limit,
        } => (target, Operation::List { after, limit }),
        ClientSecret::Retire {
            target,
            secret,
            revision,
        } => (target, Operation::Retire { secret, revision }),
    };
    Command::ClientSecret {
        target: Target {
            application: target.application,
            client: target.client,
        },
        operation,
    }
}

fn definitions(selection: DefinitionSelection) -> darkhorse_domain::operator_catalog::Definitions {
    use darkhorse_domain::operator_catalog::Definitions;
    match selection.application {
        Some(id) => Definitions::Application(id),
        None => Definitions::All,
    }
}
