//! Interface over xmtpd_cli

use std::net::SocketAddrV4;

use crate::Config;
use crate::app::ServiceManager;
use crate::services::{Service, allocate_xmtpd_port};
use crate::{
    constants::{Anvil as AnvilConst, Xmtpd as XmtpdConst},
    network::XNET_NETWORK_NAME,
    services::{ToxiProxy, ensure_image_exists},
    types::XmtpdNode,
};
use alloy::hex;
use alloy::signers::local::PrivateKeySigner;
use bollard::query_parameters::WaitContainerOptionsBuilder;
use bollard::{Docker, config::ContainerCreateBody, models::HostConfig};
use bon::Builder;
use color_eyre::eyre::Context;
use color_eyre::eyre::{OptionExt, Result};
use futures::StreamExt;
use futures::TryStreamExt;
use itertools::Itertools;
use std::io::{Read, Write, stdout};
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
#[builder(finish_fn(vis = "", name = build_internal))]
pub struct XmtpdCli {
    /// The version tag for the xmtpd-cli image (e.g., "main", "v1.0.0")
    #[builder(default = XmtpdConst::VERSION.to_string())]
    version: String,
    #[builder(default = XmtpdConst::CLI_IMAGE.to_string())]
    image: String,
    /// ToxiProxy instance for network access
    toxiproxy: ToxiProxy,
}

impl<S: xmtpd_cli_builder::IsComplete> XmtpdCliBuilder<S> {
    pub fn build(self) -> XmtpdCli {
        // Delegate to `build_internal()` to get the instance of user.
        let mut this = self.build_internal();
        let config = Config::load_unchecked();
        if let Some(version) = config.xmtpd.version {
            this.version = version;
        }
        if let Some(image) = config.xmtpd.image {
            this.image = image;
        }
        this
    }
}

impl XmtpdCli {
    // TODO: need to fill in environment variables
    pub async fn register(
        &self,
        mgr: &ServiceManager,
        w: impl Write,
        node: &XmtpdNode,
    ) -> Result<()> {
        // let host = mgr
        //     .gateway
        //     .external_url()
        //     .ok_or_eyre("no url for gateway")?;
        // info!("gateway host {host}");
        // let node = XmtpdNode::new(host.as_str()).await?;
        let port = node.port();
        let addr = node.address();
        let pubkey = hex::encode(node.compressed_public_key());
        let cmd = vec![
            "--config-file=config://anvil".into(),
            format!("--private-key={}", AnvilConst::ADMIN_KEY),
            format!("--settlement-rpc-url={}", AnvilConst::SETTLEMENT_RPC_URL),
            "nodes".to_string(),
            "register".to_string(),
            format!("--owner-address={addr}"),
            format!("--signing-key-pub=0x{pubkey}"),
            format!("--http-address=http://{}.xmtpd.local", node.name()),
        ];
        self.run(cmd, None, w).await?;
        Ok(())
    }

    pub async fn enable(&self, node: &mut XmtpdNode, w: impl Write) -> Result<()> {
        let id = node.id();
        let cmd = vec![
            "--config-file=config://anvil".into(),
            format!("--private-key={}", AnvilConst::ADMIN_KEY),
            format!("--settlement-rpc-url={}", AnvilConst::SETTLEMENT_RPC_URL),
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
        let image = format!("{}:{}", self.image, self.version);
        ensure_image_exists(&docker, &image).await?;

        info!("running xmtpd_cli {}", cmd.iter().join(" "));
        let config = ContainerCreateBody {
            image: Some(image.clone()),
            cmd: Some(cmd),
            env,
            host_config: Some(HostConfig {
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

        info!("starting container");
        docker.start_container(&id, None).await?;

        info!("Attaching to container");
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
        let opts = WaitContainerOptionsBuilder::default()
            .condition("not-running")
            .build();
        // Wait for the container to finish
        let _: Vec<_> = docker
            .wait_container(&id, Some(opts))
            .try_collect()
            .await
            .wrap_err("failed to wait for xmtpd_cli to finish")?;

        // Clean up the container
        docker
            .remove_container(&id, None)
            .await
            .wrap_err("failed to remove xmtpd_cli container")?;

        Ok(())
    }
}
