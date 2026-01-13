//! xnet - XMTP Network Testing Framework
//!
//! A CLI tool for managing Docker containers for XMTP testing.

// Allow unused code during development - this crate is a work in progress
#![allow(dead_code, unused_imports, unused_variables)]

mod config;
mod network;
mod services;
mod types;
mod xmtpd;
use color_eyre::eyre::Result;
use futures::{StreamExt, TryStreamExt, stream};

use crate::{
    network::Network,
    services::{Service, ToxiProxy},
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    println!("Hello, world!");
    // parse toml
    // launch stuff
    //
    //
    let network = Network::new().await?;
    let mut proxy = ToxiProxy::builder().build();
    proxy.start().await?;

    let mut services: Vec<Box<dyn Service>> = vec![
        Box::new(services::Anvil::builder().build()) as Box<_>,
        Box::new(services::Gateway::builder().build()) as Box<_>,
        Box::new(services::HistoryServer::builder().build()) as Box<_>,
        Box::new(services::MlsDb::builder().build()) as Box<_>,
        Box::new(services::Redis::builder().build()) as Box<_>,
        Box::new(services::ReplicationDb::builder().build()) as Box<_>,
        Box::new(services::V3Db::builder().build()) as Box<_>,
        Box::new(services::NodeGo::builder().build()) as Box<_>,
        Box::new(services::Validation::builder().build()) as Box<_>,
    ];
    let _: Vec<_> = stream::iter(services.iter_mut())
        .then(async |s| s.start(&proxy).await)
        .try_collect()
        .await?;
    network.delete_all().await?;
    Ok(())
}
