use darkhorse_adapters::{
    configuration, http,
    operator::{
        self,
        command::{self, Command},
    },
};
use std::process::ExitCode;
mod authentication;
mod maintenance;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), &'static str> {
    let command = command::parse(&std::env::args().skip(1).collect::<Vec<_>>())?;
    match command {
        Command::Serve => serve().await,
        command => operator::run(command).await,
    }
}

async fn serve() -> Result<(), &'static str> {
    #[cfg(feature = "benchmark-profiling")]
    let _profiler = darkhorse_adapters::postgres::profiling::install_signal_reporter()
        .map_err(|_| "Cannot install benchmark profiling signal handler.")?;
    let settings = configuration::load(envbind::ProcessEnvironment)
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
