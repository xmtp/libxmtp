use clap::Parser;
use std::path::PathBuf;
use xmtp_backend::{config::Config, server};

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
    let _logging = xmtp_logging::XmtpLogging::builder()
        .level(config.server.log_level.into())
        .install()?;
    let address = config.server.listen.clone();
    let backend = server::initialize(config).await?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(listen = %listener.local_addr()?, "backend ready to serve");
    server::serve(backend, listener, shutdown()).await?;
    Ok(())
}

async fn shutdown() {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("termination signal handler installs");
    tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
    tracing::info!("backend shutdown requested");
}
