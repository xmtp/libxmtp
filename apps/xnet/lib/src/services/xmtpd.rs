//! XMTPD Service
//!
//! Manages an xmtpd Docker container. Each instance is launched from an `XmtpdNode`
//! which provides the signer private key, node ID, and ToxiProxy port.

use crate::services::{ReplicationDb, wait_for_healthy_events};
use alloy::hex;
use async_trait::async_trait;
use bollard::{
    Docker,
    models::{ContainerCreateBody, HostConfig},
    query_parameters::CreateContainerOptionsBuilder,
};
use bon::Builder;
use color_eyre::eyre::OptionExt;
use color_eyre::eyre::Result;
use url::Url;

use crate::{
    constants::{
        Anvil as AnvilConst, ReplicationDb as ReplicationDbConst, Validation as ValidationConst,
        Xmtpd as XmtpdConst,
    },
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy},
    types::XmtpdNode,
};

fn default_anvil_host() -> String {
    format!("{}:{}", AnvilConst::CONTAINER_NAME, AnvilConst::PORT)
}

fn default_replication_db_host() -> String {
    format!(
        "{}:{}",
        ReplicationDbConst::CONTAINER_NAME,
        ReplicationDbConst::PORT
    )
}
fn default_validation_host() -> String {
    format!(
        "{}:{}",
        ValidationConst::CONTAINER_NAME,
        ValidationConst::PORT
    )
}
/// Manages an xmtpd Docker container.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
pub struct Xmtpd {
    /// xmtpd server image
    #[builder(default = XmtpdConst::IMAGE.to_string())]
    image: String,

    /// Version tag
    #[builder(default = XmtpdConst::VERSION.to_string())]
    version: String,

    /// Anvil host (container:port) for chain URLs
    #[builder(default = default_anvil_host())]
    anvil_host: String,

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

    #[builder(default = false)]
    migrator: bool,

    #[builder(default = false)]
    migrator_client: bool,
    migrator_client_id: Option<u32>,

    /// The XmtpdNode this service is launched from (provides signer, node_id, port)
    node: XmtpdNode,

    // --- Runtime state (skipped in builder) ---
    #[builder(default = ManagedContainer::new())]
    container: ManagedContainer,

    #[builder(skip)]
    container_name: Option<String>,

    #[builder(skip)]
    db: Option<ReplicationDb>,
}

impl Xmtpd {
    /// Start the xmtpd container.
    ///
    /// Registers itself with ToxiProxy for external access.
    /// If a container with the same name already exists, it will be reused.
    pub async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        let mut db = ReplicationDb::builder().node(self.node.clone()).build();
        let name = db.name();
        wait_for_healthy_events(db.start(), &name).await?;
        self.db = Some(db);

        let container_name = self.container_name();
        let options = CreateContainerOptionsBuilder::default()
            .name(&container_name)
            .platform("linux/amd64");

        let Self {
            node,
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

        let db_connection = self
            .db
            .as_ref()
            .ok_or_eyre("db must exist for xmtpd")?
            .url();

        let mut env = vec![
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
        if self.migrator {
            env.extend(vec![
                format!("XMTPD_MIGRATION_SERVER_ENABLE=true"),
                format!("XMTPD_MIGRATION_PAYER_PRIVATE_KEY={private_key}"),
                format!("XMTPD_MIGRATION_NODE_SIGNING_KEY={private_key}"),
                format!("XMTPD_MIGRATION_DB_READER_CONNECTION_STRING={db_connection}"),
                format!("XMTPD_MIGRATION_DB_READER_TIMEOUT=10s"),
                format!("XMTPD_MIGRATION_DB_WAIT_FOR=30s"),
                format!("XMTPD_MIGRATION_DB_BATCH_SIZE=1000"),
                format!("XMTPD_MIGRATION_DB_PROCESS_INTERVAL=10s"),
                format!("XMTPD_MIGRATION_DB_NAMESPACE=postgres"),
            ]);
        }
        if self.migrator_client {
            let id = self
                .migrator_client_id
                .ok_or_eyre("node id must be provided for migrator client")?;
            env.extend(vec![
                format!("XMTPD_MIGRATION_CLIENT_ENABLE=true"),
                format!("XMTPD_MIGRATION_CLIENT_FROM_NODE_ID={id}"),
            ])
        }
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
        let config = crate::Config::load_unchecked();
        if config.use_standard_ports && *self.node.id() == XmtpdConst::NODE_ID_INCREMENT {
            let upstream = format!("{}:{}", self.container_name(), XmtpdConst::GRPC_PORT);
            toxiproxy
                .register_at(
                    format!("xmtpd_{}_grpc", self.node.id()),
                    upstream,
                    XmtpdConst::GRPC_PORT,
                )
                .await?;
            info!(
                "registered first XMTPD node on standard port {}",
                XmtpdConst::GRPC_PORT
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
            XmtpdConst::GRPC_PORT
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
