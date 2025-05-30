mod cached_signature_verifier;
mod config;
mod handlers;
mod health_check;
mod version;

use crate::cached_signature_verifier::CachedSmartContractSignatureVerifier;
use crate::version::get_version;
use clap::Parser;
use config::Args;
use handlers::ValidationService;
use health_check::health_check_server;
use tokio::signal::unix::{signal, SignalKind};
use tonic::transport::Server;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt as _, EnvFilter};
use xmtp_id::scw_verifier::MultiSmartContractSignatureVerifier;
use xmtp_proto::xmtp::mls_validation::v1::validation_api_server::ValidationApiServer;

#[macro_use]
extern crate tracing;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let args = Args::parse();

    if args.version {
        info!("Version: {0}", get_version());
        return Ok(());
    }

    let addr = format!("0.0.0.0:{}", args.port).parse()?;
    info!("Starting validation service on port {:?}", args.port);
    info!("Starting health check on port {:?}", args.health_check_port);
    info!("Cache size: {:?}", args.cache_size);

    let health_server = health_check_server(args.health_check_port as u16);
    tracing::info!("Chain Urls: {:?}", args.chain_urls);
    let verifier = match args.chain_urls {
        Some(path) => MultiSmartContractSignatureVerifier::new_from_file(path)?,
        None => MultiSmartContractSignatureVerifier::new_from_env()?,
    };

    let cached_verifier: CachedSmartContractSignatureVerifier =
        CachedSmartContractSignatureVerifier::new(verifier, args.cache_size)?;

    let grpc_server = Server::builder()
        .add_service(ValidationApiServer::new(ValidationService::new(
            cached_verifier,
        )))
        .serve_with_shutdown(addr, async {
            wait_for_quit().await;
            info!("Shutdown signal received");
        });

    let _ = tokio::join!(health_server, grpc_server);

    Ok(())
}

pub async fn wait_for_quit() {
    let mut sigint = signal(SignalKind::interrupt()).unwrap();
    let mut sigterm = signal(SignalKind::terminate()).unwrap();
    tokio::select! {
        _ = sigint.recv() => (),
        _ = sigterm.recv() => (),
    };
}
