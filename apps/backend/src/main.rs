use clap::Parser;
use std::path::PathBuf;
use xmtp_backend::{config::Config, server};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    xmtp_cryptography::install_crypto_provider();
    let args = Args::parse();
    let _logging = xmtp_logging::XmtpLogging::builder().install()?;
    let config = Config::load(&args.config)?;
    let address = config.server.listen.clone();
    let backend = server::initialize(config).await?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    server::serve(backend, listener, shutdown()).await?;
    Ok(())
}

async fn shutdown() {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("termination signal handler installs");
    tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
}
