mod config;
mod handlers;
mod health_check;

use clap::Parser;
use config::Args;
use env_logger::Env;
use handlers::ValidationService;
use health_check::health_check_server;
use tokio::signal::unix::{signal, SignalKind};
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
    info!("Starting validation service on port {:?}", args.port);
    info!("Starting health check on port {:?}", args.health_check_port);

    let health_server = health_check_server(args.health_check_port as u16);

    let grpc_server = Server::builder()
        .add_service(ValidationApiServer::new(ValidationService::default()))
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
