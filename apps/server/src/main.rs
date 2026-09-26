use darkhorse_adapters::{
    configuration, http,
    operator::{self, command::Command},
};
use operator::localization::Locale;
use std::process::ExitCode;
mod authentication;
mod maintenance;

fn main() -> ExitCode {
    use operator::{
        cli,
        output::{self, Format},
    };
    let args = std::env::args_os()
        .skip(1)
        .take(cli::ARGUMENT_LIMIT + 1)
        .collect::<Vec<_>>();
    let presentation = match cli::presentation(&args) {
        Ok(presentation) => presentation,
        Err(error) => return finish(Err(error), Format::Human, Locale::English),
    };
    let locale = match match presentation.locale {
        Some(locale) => Ok(locale),
        None => operator::localization::load(envbind::ProcessEnvironment),
    } {
        Ok(locale) => locale,
        Err(error) => return finish(Err(error), Format::Human, Locale::English),
    };
    match cli::invocation_in(&args, locale) {
        // Existing argument failures are English text, including in JSON mode.
        // Keep that historical byte contract as well as runtime JSON records.
        Err(error) => finish(
            Err(error),
            Format::Human,
            if presentation.format == Format::Json {
                Locale::English
            } else {
                locale
            },
        ),
        Ok(cli::Plan::Display(text)) => finish(output::display(&text), Format::Human, locale),
        Ok(cli::Plan::Run(invocation)) => finish(
            execute(
                invocation.command,
                invocation.confirmed,
                invocation.format,
                invocation.auth_stdin,
                locale,
            ),
            invocation.format,
            locale,
        ),
    }
}
fn finish(
    result: Result<(), operator::output::Failure>,
    format: operator::output::Format,
    locale: Locale,
) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => match operator::output::diagnose_in(&error, format, locale) {
            Ok(()) => ExitCode::from(error.exit_code()),
            Err(write) => ExitCode::from(write.exit_code()),
        },
    }
}
fn execute(
    command: Command,
    confirmed: bool,
    format: operator::output::Format,
    auth_stdin: bool,
    locale: Locale,
) -> Result<(), operator::output::Failure> {
    operator::confirmation::confirm(&command, confirmed, format, auth_stdin, locale)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|_| "Cannot initialize runtime.")?;
    runtime.block_on(dispatch(command, format, auth_stdin, locale))
}
async fn dispatch(
    command: Command,
    format: operator::output::Format,
    auth_stdin: bool,
    locale: Locale,
) -> Result<(), operator::output::Failure> {
    match command {
        Command::Serve => serve().await.map_err(Into::into),
        command => operator::output::emit_in(
            &operator::run(command, auth_stdin, locale).await?,
            format,
            locale,
        ),
    }
}

async fn serve() -> Result<(), &'static str> {
    #[cfg(feature = "benchmark-profiling")]
    let _profiler = darkhorse_adapters::postgres::profiling::install_signal_reporter()
        .map_err(|_| "Cannot install benchmark profiling signal handler.")?;
    let settings =
        configuration::load(darkhorse_adapters::deployment_environment::DeploymentEnvironment)
            .map_err(|_| "Invalid server configuration; check DARKHORSE_* settings.")?;
    let presentation = darkhorse_adapters::localization_http::router(
        settings.default_locale,
        settings.public_origin.clone(),
    );
    let authentication = authentication::runtime(&settings).await?;
    let listener = tokio::net::TcpListener::bind(settings.listen)
        .await
        .map_err(|_| "Cannot bind HTTP listener; check host and port availability.")?;
    let server = axum::serve(
        listener,
        http::with_authentication(
            settings.static_dir,
            authentication.router.merge(presentation),
        ),
    )
    .with_graceful_shutdown(shutdown());
    tokio::select! {
        result = std::future::IntoFuture::into_future(server) => result.map_err(|_| "HTTP server stopped unexpectedly."),
        _ = maintenance::media(authentication.media) => Err("Media maintenance stopped unexpectedly."),
        _ = maintenance::email(authentication.email) => Err("Email maintenance stopped unexpectedly."),
        _ = maintenance::run(authentication.maintenance) => Err("Credential maintenance stopped unexpectedly."),
    }
}

async fn shutdown() {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("signal handler installation failed");
    tokio::select! {
        result = tokio::signal::ctrl_c() => { result.expect("signal handler installation failed"); }
        _ = terminate.recv() => {}
    }
}
