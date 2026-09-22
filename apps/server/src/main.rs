use darkhorse_adapters::{
    configuration, http,
    operator::{self, command::Command},
};
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
    match cli::invocation(&args) {
        Err(error) => finish(Err(error), Format::Human),
        Ok(cli::Plan::Display(text)) => finish(output::display(&text), Format::Human),
        Ok(cli::Plan::Run(invocation)) => finish(
            execute(invocation.command, invocation.confirmed, invocation.format),
            invocation.format,
        ),
    }
}
fn finish(
    result: Result<(), operator::output::Failure>,
    format: operator::output::Format,
) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => match operator::output::diagnose(&error, format) {
            Ok(()) => ExitCode::from(error.exit_code()),
            Err(write) => ExitCode::from(write.exit_code()),
        },
    }
}
fn execute(
    command: Command,
    confirmed: bool,
    format: operator::output::Format,
) -> Result<(), operator::output::Failure> {
    operator::confirmation::confirm(command, confirmed, format)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|_| "Cannot initialize runtime.")?;
    runtime.block_on(dispatch(command, format))
}
async fn dispatch(
    command: Command,
    format: operator::output::Format,
) -> Result<(), operator::output::Failure> {
    match command {
        Command::Serve => serve().await.map_err(Into::into),
        command => operator::output::emit(&operator::run(command).await?, format),
    }
}

async fn serve() -> Result<(), &'static str> {
    #[cfg(feature = "benchmark-profiling")]
    let _profiler = darkhorse_adapters::postgres::profiling::install_signal_reporter()
        .map_err(|_| "Cannot install benchmark profiling signal handler.")?;
    let settings =
        configuration::load(darkhorse_adapters::deployment_environment::DeploymentEnvironment)
            .map_err(|_| "Invalid server configuration; check DARKHORSE_* settings.")?;
    let authentication = authentication::runtime(&settings).await?;
    let listener = tokio::net::TcpListener::bind(settings.listen)
        .await
        .map_err(|_| "Cannot bind HTTP listener; check host and port availability.")?;
    let server = axum::serve(
        listener,
        http::with_authentication(settings.static_dir, authentication.router),
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
