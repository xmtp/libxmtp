mod config;
mod handlers;
mod validation_helpers;

use clap::Parser;
use config::Args;
use env_logger::Env;
use handlers::ValidationService;
use tokio::{
    signal::unix::{signal, SignalKind},
    spawn,
    sync::oneshot::{self, Sender},
};
use tonic::transport::Server;
use xmtp_proto::xmtp::mls_validation::v1::validation_api_server::ValidationApiServer;

#[macro_use]
extern crate log;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = Env::default();
    env_logger::init_from_env(env);

    let args = Args::parse();
    let addr = format!("0.0.0.0:{}", args.port).parse()?;
    info!("Starting validation service on port {:?}", addr);

    let (signal_tx, signal_rx) = oneshot::channel();
    spawn(wait_for_sigint(signal_tx));

    Server::builder()
        .add_service(ValidationApiServer::new(ValidationService::default()))
        .serve_with_shutdown(addr, async {
            signal_rx.await.ok();
            info!("Shutdown signal received");
        })
        .await?;

    Ok(())
}

async fn wait_for_sigint(tx: Sender<()>) {
    // I was having shutdown problems without adding this helper
    // The thing just refused to die locally
    let _ = signal(SignalKind::interrupt())
        .expect("failed to install signal handler")
        .recv()
        .await;
    println!("SIGINT received: shutting down");
    let _ = tx.send(());
}
