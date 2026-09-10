use clap::Parser;
use std::path::PathBuf;
use xmtp_backend::{
    config::{Config, LogFormat},
    server, telemetry,
};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    config: PathBuf,
    /// Override server.log_level from the configuration file.
    #[arg(long, value_enum)]
    log_level: Option<xmtp_backend::config::LogLevel>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    xmtp_cryptography::install_crypto_provider();
    let args = Args::parse();
    let mut config = Config::load(&args.config)?;
    if let Some(level) = args.log_level {
        config.server.log_level = level;
    }
    run(config, shutdown()).await
}

/// Install telemetry before database startup, then flush export after the transport drain.
async fn run(
    config: Config,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let otlp = config.telemetry.logging_config()?;
    telemetry::install(&config.telemetry.metrics_listen)?;
    let stdout_level: xmtp_logging::Level = config.server.log_level.into();
    let level = match stdout_level {
        xmtp_logging::Level::Debug | xmtp_logging::Level::Trace => stdout_level,
        _ => xmtp_logging::Level::Info,
    };
    let logging = xmtp_logging::XmtpLogging::builder()
        .level(level)
        .stdout_level(stdout_level)
        .json(config.server.log_format == LogFormat::Json)
        .with_telemetry(otlp);
    let logging = logging.install()?;
    xmtp_logging::propagation::install();
    telemetry::describe();
    telemetry::ready(false);
    telemetry::info(env!("CARGO_PKG_VERSION"));
    let address = config.server.listen.clone();
    let backend = server::initialize(config).await?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(listen = %listener.local_addr()?, "backend ready to serve");
    let result = server::serve(backend, listener, shutdown).await;
    logging.disable_telemetry()?;
    result?;
    Ok(())
}

async fn shutdown() {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("termination signal handler installs");
    tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
    tracing::info!("backend shutdown requested");
}

#[cfg(test)]
#[path = "main/tests.rs"]
mod tests;
