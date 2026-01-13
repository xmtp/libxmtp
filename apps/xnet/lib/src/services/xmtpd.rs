//! XMTPD Service
//!
//! Manages an xmtpd Docker container. Each instance is launched from an `XmtpdNode`
//! which provides the signer private key, node ID, and ToxiProxy port.

use alloy::hex;
use async_trait::async_trait;
use bollard::{
    Docker,
    models::{ContainerCreateBody, HostConfig},
    query_parameters::CreateContainerOptionsBuilder,
};
use bon::Builder;
use color_eyre::eyre::Result;
use url::Url;

use crate::{
    constants::{
        ANVIL_CONTAINER_NAME, ANVIL_PORT, DEFAULT_XMTPD_IMAGE, DEFAULT_XMTPD_VERSION,
        POSTGRES_PORT, REPLICATION_DB_CONTAINER_NAME, VALIDATION_CONTAINER_NAME, VALIDATION_PORT,
        XMTPD_GRPC_PORT, XMTPD_NODE_ID_INCREMENT,
    },
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy},
    types::XmtpdNode,
};

fn default_anvil_host() -> String {
    format!("{ANVIL_CONTAINER_NAME}:{ANVIL_PORT}")
}

fn default_replication_db_host() -> String {
    format!("{REPLICATION_DB_CONTAINER_NAME}:{POSTGRES_PORT}")
}
fn default_validation_host() -> String {
    format!("{VALIDATION_CONTAINER_NAME}:{VALIDATION_PORT}")
}
/// Manages an xmtpd Docker container.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
pub struct Xmtpd {
    /// xmtpd server image
    #[builder(default = DEFAULT_XMTPD_IMAGE.to_string())]
    image: String,

    /// Version tag
    #[builder(default = DEFAULT_XMTPD_VERSION.to_string())]
    version: String,

    /// Anvil host (container:port) for chain URLs
    #[builder(default = default_anvil_host())]
    anvil_host: String,

    /// Replication DB host (container:port) for writer connection
    #[builder(default = default_replication_db_host())]
    db_host: String,

    #[builder(default = default_validation_host())]
    validation_host: String,

    /// Contracts environment name
    #[builder(default = "anvil".to_string())]
    contracts_environment: String,

    /// Log level
    #[builder(default = "debug".to_string())]
    log_level: String,

    /// Enable gRPC reflection
    #[builder(default = true)]
    reflection_enable: bool,

    /// The XmtpdNode this service is launched from (provides signer, node_id, port)
    node: XmtpdNode,

    // --- Runtime state (skipped in builder) ---
    #[builder(default = ManagedContainer::new())]
    container: ManagedContainer,

    #[builder(skip)]
    container_name: Option<String>,
}

impl Xmtpd {
    /// Start the xmtpd container.
    ///
    /// Registers itself with ToxiProxy for external access.
    /// If a container with the same name already exists, it will be reused.
    pub async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        let container_name = self.container_name();

        let options = CreateContainerOptionsBuilder::default()
            .name(&container_name)
            .platform("linux/amd64");

        let Self {
            node,
            db_host,
            contracts_environment,
            anvil_host,
            log_level,
            reflection_enable,
            image,
            version,
            validation_host,
            ..
        } = self;
        let private_key = format!("0x{}", hex::encode(node.signer().credential().to_bytes()));

        let db_connection = format!("postgres://postgres:xmtp@{db_host}/postgres?sslmode=disable",);

        let env = vec![
            format!("XMTPD_SIGNER_PRIVATE_KEY={private_key}"),
            format!("XMTPD_PAYER_PRIVATE_KEY={private_key}"),
            "XMTPD_REPLICATION_ENABLE=true".to_string(),
            "XMTPD_INDEXER_ENABLE=true".to_string(),
            "XMTPD_SYNC_ENABLE=true".to_string(),
            format!("XMTPD_CONTRACTS_ENVIRONMENT={contracts_environment}"),
            format!("XMTPD_DB_WRITER_CONNECTION_STRING={db_connection}"),
            format!("XMTPD_APP_CHAIN_RPC_URL=http://{anvil_host}"),
            format!("XMTPD_APP_CHAIN_WSS_URL=ws://{anvil_host}"),
            format!("XMTPD_SETTLEMENT_CHAIN_RPC_URL=http://{anvil_host}"),
            format!("XMTPD_SETTLEMENT_CHAIN_WSS_URL=ws://{anvil_host}"),
            format!("XMTPD_LOG_LEVEL={log_level}"),
            format!("XMTPD_REFLECTION_ENABLE={reflection_enable}"),
        ];
        let config = ContainerCreateBody {
            image: Some(format!("{image}:{version}")),
            cmd: Some(vec![format!(
                "--mls-validation.grpc-address=http://{validation_host}"
            )]),
            env: Some(env),
            host_config: Some(HostConfig {
                network_mode: Some(XNET_NETWORK_NAME.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        self.container
            .start_container(&container_name, options, config)
            .await?;
        self.container_name = Some(container_name);

        let port = self.register(toxiproxy, None).await?;
        self.container.set_proxy_port(port);

        // When using standard ports, also expose the first XMTPD node on localhost:5050
        // this is for compatiblity with libxmtp tests, some of which expect
        // xmtpd to always be at localhost:5050
        let config = crate::Config::load_unchecked();
        if config.use_standard_ports && *self.node.id() == XMTPD_NODE_ID_INCREMENT {
            let upstream = format!("{}:{}", self.container_name(), XMTPD_GRPC_PORT);
            toxiproxy
                .register_at(
                    format!("xmtpd_{}_grpc", self.node.id()),
                    upstream,
                    XMTPD_GRPC_PORT,
                )
                .await?;
            info!(
                "registered first XMTPD node on standard port {}",
                XMTPD_GRPC_PORT
            );
        }

        Ok(())
    }

    /// Stop the xmtpd container.
    pub async fn stop(&mut self) -> Result<()> {
        let name = self.container_name();
        self.container.stop_container(&name).await
    }

    /// Get the hostname for this XMTPD node (for unified addressing).
    pub fn hostname(&self) -> String {
        format!("node{}.xmtpd.local", self.node.id())
    }

    /// Internal URL for use within the docker network.
    /// Returns container address for ToxiProxy upstream registration.
    pub fn url(&self) -> Url {
        Url::parse(&format!(
            "http://{}:{}",
            self.container_name(),
            XMTPD_GRPC_PORT
        ))
        .expect("valid URL")
    }

    /// External URL for access through ToxiProxy.
    /// Returns hostname for unified addressing.
    pub fn external_url(&self) -> Url {
        Url::parse(&format!("http://{}", self.hostname())).expect("valid URL")
    }

    /// Container name derived from the node ID.
    pub fn container_name(&self) -> String {
        format!("xnet-{}", self.node.id())
    }

    /// Check if xmtpd is running.
    pub fn is_running(&self) -> bool {
        self.container.is_running()
    }

    /// Get the ToxiProxy port for this service.
    pub fn proxy_port(&self) -> Option<u16> {
        self.container.proxy_port()
    }
}

#[async_trait]
impl Service for Xmtpd {
    async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        Xmtpd::start(self, toxiproxy).await
    }

    async fn stop(&mut self) -> Result<()> {
        Xmtpd::stop(self).await
    }

    fn is_running(&self) -> bool {
        Xmtpd::is_running(self)
    }

    fn url(&self) -> Url {
        Xmtpd::url(self)
    }

    fn external_url(&self) -> Url {
        Xmtpd::external_url(self)
    }

    fn name(&self) -> String {
        format!("xmtpd_{}", self.node.id())
    }

    fn port(&self) -> u16 {
        self.node.port()
    }

    fn hostname(&self) -> Option<String> {
        Some(Xmtpd::hostname(self))
    }
}
