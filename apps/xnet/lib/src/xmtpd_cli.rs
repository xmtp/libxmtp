//! Interface over xmtpd_cli

use std::net::SocketAddrV4;

use crate::app::ServiceManager;
use crate::services::{allocate_xmtpd_port, Service};
use crate::{
    constants::{
        ANVIL_ADMIN_KEY, DEFAULT_XMTPD_CLI_IMAGE, DEFAULT_XMTPD_VERSION, SETTLEMENT_RPC_URL,
        TOXIPROXY_CONTAINER_NAME,
    },
    network::XNET_NETWORK_NAME,
    services::{ensure_image_exists, ToxiProxy},
    types::XmtpdNode,
};
use alloy::hex;
use alloy::signers::local::PrivateKeySigner;
use bollard::{config::ContainerCreateBody, models::HostConfig, Docker};
use bon::Builder;
use color_eyre::eyre::Context;
use color_eyre::eyre::{OptionExt, Result};
use futures::StreamExt;
use futures::TryStreamExt;
use std::io::{stdout, Read, Write};
use std::net::Ipv4Addr;
use std::net::TcpListener;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::time::sleep;
use tracing::info;
use xmtp_api_d14n::d14n::GetNodes;
use xmtp_proto::api::Query;

#[derive(Builder, Clone)]
#[builder(on(String, into), derive(Debug, Clone))]
pub struct XmtpdCli {
    /// The version tag for the xmtpd-cli image (e.g., "main", "v1.0.0")
    #[builder(default = DEFAULT_XMTPD_VERSION.to_string())]
    version: String,
    /// ToxiProxy instance for network access
    toxiproxy: ToxiProxy,
}

impl XmtpdCli {
    // TODO: need to fill in environment variables
    pub async fn register(&self, mgr: &ServiceManager, w: impl Write) -> Result<XmtpdNode> {
        let host = mgr
            .gateway
            .external_url()
            .ok_or_eyre("no url for gateway")?;
        info!("gateway host {host}");
        let node = XmtpdNode::new(host.as_str()).await?;
        let port = node.port();
        let addr = node.address();
        let pubkey = hex::encode(node.compressed_public_key());
        let cmd = vec![
            "--config-file=config://anvil".into(),
            format!("--private-key={ANVIL_ADMIN_KEY}"),
            format!("--settlement-rpc-url={SETTLEMENT_RPC_URL}"),
            "nodes".to_string(),
            "register".to_string(),
            format!("--owner-address={addr}"),
            format!("--signing-key-pub=0x{pubkey}"),
            format!("--http-address=http://node{}.xmtpd.local", node.id()),
        ];
        self.run(cmd, None, w).await?;
        Ok(node)
    }

    pub async fn enable(&self, node: &mut XmtpdNode, w: impl Write) -> Result<()> {
        let id = node.id();
        let cmd = vec![
            "--config-file=config://anvil".into(),
            format!("--private-key={ANVIL_ADMIN_KEY}"),
            format!("--settlement-rpc-url={SETTLEMENT_RPC_URL}"),
            "nodes".to_string(),
            "canonical-network".to_string(),
            "--add".to_string(),
            format!("--node-id={id}"),
        ];
        self.run(cmd, None, w).await?;
        Ok(())
    }

    /// Run a single command in a temporary container that auto-removes after completion.
    async fn run(
        &self,
        cmd: Vec<String>,
        env: Option<Vec<String>>,
        mut write: impl Write,
    ) -> Result<()> {
        let docker = Docker::connect_with_socket_defaults()?;
        let image = format!("{}:{}", DEFAULT_XMTPD_CLI_IMAGE, self.version);
        ensure_image_exists(&docker, &image).await?;

        let config = ContainerCreateBody {
            image: Some(image.clone()),
            cmd: Some(cmd),
            env: env,
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

        let bollard::container::AttachContainerResults { mut output, .. } = docker
            .attach_container(
                &id,
                Some(
                    bollard::query_parameters::AttachContainerOptionsBuilder::default()
                        .stdout(true)
                        .stderr(true)
                        .stdin(false)
                        .stream(true)
                        .build(),
                ),
            )
            .await?;
        // pipe docker attach output into stdout
        while let Some(Ok(output)) = output.next().await {
            write.write_all(output.into_bytes().as_ref())?;
            write.flush()?;
        }
        // Wait for the container to finish
        let _: Vec<_> = docker
            .wait_container(&id, None::<bollard::query_parameters::WaitContainerOptions>)
            .try_collect()
            .await
            .wrap_err("failed to wait for xmtpd_cli to finish")?;

        Ok(())
    }
}
