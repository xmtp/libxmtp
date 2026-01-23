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
use futures::{FutureExt, StreamExt, TryStreamExt, stream};

use crate::{
    network::Network,
    services::{Service, ToxiProxy},
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();

    // parse toml
    // launch stuff
    let network = Network::new().await?;
    let mut proxy = ToxiProxy::builder().build();
    proxy.start().await?;

    let mut services: Vec<Box<dyn Service>> = vec![
        Box::new(services::Anvil::builder().build()) as Box<_>,
        Box::new(services::Gateway::builder().build()) as Box<_>,
        Box::new(services::HistoryServer::builder().build()) as Box<_>,
        Box::new(services::Redis::builder().build()) as Box<_>,
        Box::new(services::ReplicationDb::builder().build()) as Box<_>,
    ];
    start_v3(&proxy).await?;
    let _: Vec<_> = stream::iter(services.iter_mut())
        .then(async |s| s.start(&proxy).await)
        .try_collect()
        .await?;
    tracing::info!("deleting network in 60s");
    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    network.delete_all().await?;
    Ok(())
}

async fn start_d14n(proxy: &ToxiProxy) -> Result<Vec<Box<dyn Service>>> {
    let mut anvil = services::Anvil::builder().build();
    let mut redis = services::Redis::builder().build();
    let mut replication_db = services::ReplicationDb::builder().build();

    let launch = vec![
        anvil.start(&proxy).boxed_local(),
        redis.start(&proxy).boxed_local(),
        replication_db.start(&proxy).boxed_local(),
    ];
    futures::future::try_join_all(launch).await?;
    let mut gateway = services::Gateway::builder()
        .redis_host(redis.internal_proxy_host()?)
        .build();
    gateway.start(proxy).await?;
    Ok(vec![
        Box::new(anvil) as _,
        Box::new(gateway) as _,
        Box::new(redis) as _,
        Box::new(replication_db) as _,
    ])
}

async fn start_v3(proxy: &ToxiProxy) -> Result<Vec<Box<dyn Service>>> {
    let mut validation = services::Validation::builder().build();
    let mut mls_db = services::MlsDb::builder().build();
    let mut v3_db = services::V3Db::builder().build();
    // history is both but OK to start with v3 stuff
    let mut history = services::HistoryServer::builder().build();
    // dependencies
    let launch = vec![
        validation.start(proxy).boxed_local(),
        mls_db.start(proxy).boxed_local(),
        v3_db.start(proxy).boxed_local(),
        history.start(&proxy).boxed_local(),
    ];
    futures::future::try_join_all(launch).await?;
    let mut node_go = services::NodeGo::builder()
        .store_db_host(v3_db.internal_proxy_host()?)
        .mls_store_db_host(mls_db.internal_proxy_host()?)
        .mls_validation_address(validation.internal_proxy_host()?)
        .build();
    node_go.start(proxy).await?;

    Ok(vec![
        Box::new(validation) as _,
        Box::new(mls_db) as _,
        Box::new(v3_db) as _,
        Box::new(node_go) as _,
        Box::new(history) as _,
    ])
}
