use darkhorse_adapters::{
    configuration, http,
    operator::{
        self,
        command::{self, Command},
    },
};
use std::process::ExitCode;

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
    let settings = configuration::load(envbind::ProcessEnvironment)
        .map_err(|_| "Invalid server configuration; check DARKHORSE_* settings.")?;
    let listener = tokio::net::TcpListener::bind(settings.listen)
        .await
        .map_err(|_| "Cannot bind HTTP listener; check host and port availability.")?;
    axum::serve(listener, http::router(settings.static_dir))
        .with_graceful_shutdown(shutdown())
        .await
        .map_err(|_| "HTTP server stopped unexpectedly.")
}

async fn shutdown() {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("signal handler installation failed");
    tokio::select! {
        result = tokio::signal::ctrl_c() => { result.expect("signal handler installation failed"); }
        _ = terminate.recv() => {}
    }
}
