//! Interface over xmtpd_cli

use std::net::SocketAddrV4;

use alloy::signers::local::PrivateKeySigner;
use bollard::{Docker, config::ContainerCreateBody, models::HostConfig};
use bon::Builder;
use color_eyre::eyre::Context;
use color_eyre::eyre::{OptionExt, Result};
use futures::StreamExt;
use futures::TryStreamExt;
use std::net::Ipv4Addr;
use std::net::TcpListener;
use tracing::info;

use crate::{
    config::{ANVIL_ADMIN_KEY, DEFAULT_XMTPD_CLI_IMAGE, DEFAULT_XMTPD_VERSION, SETTLEMENT_RPC_URL},
    network::XNET_NETWORK_NAME,
    services::{ToxiProxy, ensure_image_exists},
    types::XmtpdNode,
};

#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
pub struct Xmtpd {
    /// The version tag for the xmtpd-cli image (e.g., "main", "v1.0.0")
    #[builder(default = DEFAULT_XMTPD_VERSION.to_string())]
    version: String,
    /// ToxiProxy instance for network access
    toxiproxy: ToxiProxy,
}

impl Xmtpd {
    // TODO: need to fill in environment variables
    pub async fn register(&self) -> Result<XmtpdNode> {
        let owner = PrivateKeySigner::random();
        let addr = owner.address();
        let pubkey = owner.public_key();
        let port = ask_free_tcp_port().ok_or_eyre("unable to acquire free port from OS")?;
        let cmd = vec![
            format!("--private-key={ANVIL_ADMIN_KEY}"),
            format!("--rpc_url={SETTLEMENT_RPC_URL}"),
            "nodes".to_string(),
            "register".to_string(),
            format!("--owner-address={addr}"),
            format!("--signing-key-pub={pubkey}"),
            format!("--http-address=localhost:{port}"),
        ];
        self.run(cmd).await?;
        Ok(XmtpdNode::new(port, owner))
    }

    pub async fn enable(&self, node: &XmtpdNode) -> Result<()> {
        let cmd = vec![
            format!("--private-key={ANVIL_ADMIN_KEY}"),
            format!("--rpc_url={SETTLEMENT_RPC_URL}"),
            "nodes".to_string(),
            "canonical-network".to_string(),
            "--add".to_string(),
            "--node-id=100".to_string(),
        ];
        self.run(cmd).await?;
        Ok(())
    }

    /// Run a single command in a temporary container that auto-removes after completion.
    async fn run(&self, cmd: Vec<String>) -> Result<()> {
        let docker = Docker::connect_with_socket_defaults()?;
        let image = format!("{}:{}", DEFAULT_XMTPD_CLI_IMAGE, self.version);
        ensure_image_exists(&docker, &image).await?;

        let config = ContainerCreateBody {
            image: Some(image.clone()),
            cmd: Some(cmd),
            host_config: Some(HostConfig {
                auto_remove: Some(true),
                network_mode: Some(XNET_NETWORK_NAME.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        info!("Running xmtpd-cli command");
        let id = docker
            .create_container(Default::default(), config)
            .await?
            .id;

        docker.start_container(&id, None).await?;

        // Wait for the container to finish
        let _: Vec<_> = docker
            .wait_container(&id, None::<bollard::query_parameters::WaitContainerOptions>)
            .try_collect()
            .await
            .wrap_err("failed to wait for xmtpd_cli to finish")?;

        Ok(())
    }
}

/// ask OS for a free TCP Port
fn ask_free_tcp_port() -> Option<u16> {
    let ipv4 = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0);
    Some(TcpListener::bind(ipv4).ok()?.local_addr().ok()?.port())
}
