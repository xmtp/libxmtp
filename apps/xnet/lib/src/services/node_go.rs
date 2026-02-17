//! Node-Go (XMTP node) container management.

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
    Config,
    constants::{
        MlsDb as MlsDbConst, NodeGo as NodeGoConst, POSTGRES_PASSWORD, V3Db as V3DbConst,
        Validation as ValidationConst,
    },
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy},
};

/// Manages a Node-Go (XMTP node) Docker container.
#[derive(Builder, Debug, Clone)]
#[builder(on(String, into), derive(Debug))]
#[builder(finish_fn(vis = "", name = build_internal))]
pub struct NodeGo {
    /// The image name (e.g., "ghcr.io/xmtp/node-go")
    #[builder(default = NodeGoConst::IMAGE.to_string())]
    image: String,

    /// The version tag for the node-go image (e.g., "main", "v1.0.0")
    #[builder(default = NodeGoConst::VERSION.to_string())]
    version: String,

    /// Node key for the node
    #[builder(default = NodeGoConst::NODE_KEY.to_string())]
    node_key: String,

    /// Store database connection string in the format container:port
    #[builder(default = default_store_db_host())]
    store_db_host: String,

    /// MLS store database connection string in the format container:port
    #[builder(default = default_mls_store_db_connection_string())]
    mls_store_db_host: String,

    /// MLS validation gRPC address
    #[builder(default = default_mls_validation_address())]
    mls_validation_address: String,

    /// d14n cutover timestamp in nanoseconds
    #[builder(default = i64::MAX)]
    d14n_cutover_ns: i64,

    /// Wait for database timeout
    #[builder(default = "30s".to_string())]
    wait_for_db: String,

    /// Managed container state
    #[builder(default = ManagedContainer::new())]
    container: ManagedContainer,

    /// ToxiProxy port for gRPC API (set after registering with ToxiProxy)
    #[builder(skip)]
    grpc_proxy_port: Option<u16>,

    /// ToxiProxy port for HTTP API (set after registering with ToxiProxy)
    #[builder(skip)]
    http_proxy_port: Option<u16>,
}

impl<S: node_go_builder::IsComplete> NodeGoBuilder<S> {
    pub fn build(self) -> Result<NodeGo> {
        // Delegate to `build_internal()` to get the instance of user.
        let mut this = self.build_internal();
        let config = Config::load()?;
        if let Some(version) = config.v3.version {
            this.version = version;
        }
        if let Some(image) = config.v3.image {
            this.image = image;
        }
        if let Some(ts) = config.migration.migration_timestamp {
            this.d14n_cutover_ns = ts as i64;
        }
        if let Some(port) = config.v3_port
            && !config.use_standard_ports
        {
            this.grpc_proxy_port = Some(port);
        }
        Ok(this)
    }
}
fn default_store_db_host() -> String {
    format!("{}:{}", V3DbConst::CONTAINER_NAME, V3DbConst::PORT)
}

fn default_mls_store_db_connection_string() -> String {
    format!("{}:{}", MlsDbConst::CONTAINER_NAME, MlsDbConst::PORT)
}

fn default_mls_validation_address() -> String {
    format!(
        "{}:{}",
        ValidationConst::CONTAINER_NAME,
        ValidationConst::PORT
    )
}

impl NodeGo {
    /// Start the node-go container.
    ///
    /// Registers itself with ToxiProxy for external access (both gRPC and HTTP).
    /// If a container with the same name already exists, it will be reused.
    pub async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        let NodeGo {
            store_db_host,
            mls_store_db_host,
            mls_validation_address,
            wait_for_db,
            node_key,
            image,
            version,
            ..
        } = &self;

        let options = CreateContainerOptionsBuilder::default()
            .name(NodeGoConst::CONTAINER_NAME)
            .platform("linux/amd64");

        let db_connection_str = super::db_connection_string(POSTGRES_PASSWORD, store_db_host);
        let mls_db_connection_str =
            super::db_connection_string(POSTGRES_PASSWORD, mls_store_db_host);
        info!("Connection strings: {db_connection_str}, {mls_db_connection_str}");
        let cmd = vec![
            "--store.enable".to_string(),
            format!("--store.db-connection-string={db_connection_str}",),
            format!("--store.reader-db-connection-string={db_connection_str}",),
            format!("--mls-store.db-connection-string={mls_db_connection_str}",),
            format!("--mls-validation.grpc-address={mls_validation_address}",),
            format!("--api.enable-migration"),
            format!("--api.d14n-cutover-ns={}", self.d14n_cutover_ns),
            "--api.enable-mls".to_string(),
            format!("--wait-for-db={wait_for_db}"),
        ];

        let config = ContainerCreateBody {
            image: Some(format!("{image}:{version}")),
            cmd: Some(cmd),
            env: Some(vec![format!("GOWAKU-NODEKEY={node_key}")]),
            host_config: Some(HostConfig {
                network_mode: Some(XNET_NETWORK_NAME.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        self.container
            .start_container(NodeGoConst::CONTAINER_NAME, options, config)
            .await?;

        let grpc_port = if let Some(grpc_port) = self.grpc_proxy_port {
            self.register(toxiproxy, Some(grpc_port)).await?;
            grpc_port
        } else {
            self.register(toxiproxy, None).await?
        };
        self.grpc_proxy_port = Some(grpc_port);

        // the http is not used and will be gone in d14n so we're not giving it a standard port
        let http_upstream = format!(
            "{}:{}",
            NodeGoConst::CONTAINER_NAME,
            NodeGoConst::API_HTTP_PORT
        );
        let http_port = toxiproxy.register("node_go_http", http_upstream).await?;
        self.http_proxy_port = Some(http_port);

        Ok(())
    }

    /// Reload the node-go container with a new d14n cutover timestamp.
    ///
    /// Stops and removes the existing container, then starts a fresh one
    /// with the updated `--d14n-cutover-ns` flag.
    pub async fn reload(&mut self, d14n_cutover_ns: i64, toxiproxy: &ToxiProxy) -> Result<()> {
        self.d14n_cutover_ns = d14n_cutover_ns;
        self.container
            .remove_container(NodeGoConst::CONTAINER_NAME)
            .await?;
        self.start(toxiproxy).await
    }

    pub fn mls_db_reader(&self) -> Url {
        super::db_connection_string(POSTGRES_PASSWORD, &self.mls_store_db_host)
    }

    /// Stop the node-go container.
    pub async fn stop(&mut self) -> Result<()> {
        self.container
            .stop_container(NodeGoConst::CONTAINER_NAME)
            .await
    }

    /// gRPC API URL for use within the docker network.
    pub fn grpc_url(&self) -> Url {
        Url::parse(&format!(
            "http://{}:{}",
            NodeGoConst::CONTAINER_NAME,
            NodeGoConst::API_PORT
        ))
        .expect("valid URL")
    }

    /// gRPC API URL for external access (through ToxiProxy).
    pub fn external_grpc_url(&self) -> Option<Url> {
        self.grpc_proxy_port
            .map(|port| Url::parse(&format!("http://localhost:{}", port)).expect("valid URL"))
    }

    /// HTTP API URL for use within the docker network.
    pub fn http_url(&self) -> Url {
        Url::parse(&format!(
            "http://{}:{}",
            NodeGoConst::CONTAINER_NAME,
            NodeGoConst::API_HTTP_PORT
        ))
        .expect("valid URL")
    }

    /// HTTP API URL for external access (through ToxiProxy).
    pub fn external_http_url(&self) -> Option<Url> {
        self.http_proxy_port
            .map(|port| Url::parse(&format!("http://localhost:{}", port)).expect("valid URL"))
    }

    /// Get the ToxiProxy port for the gRPC API.
    pub fn grpc_proxy_port(&self) -> Option<u16> {
        self.grpc_proxy_port
    }

    /// Get the ToxiProxy port for the HTTP API.
    pub fn http_proxy_port(&self) -> Option<u16> {
        self.http_proxy_port
    }

    /// Check if node-go is running.
    pub fn is_running(&self) -> bool {
        self.container.is_running()
    }
}

#[async_trait]
impl Service for NodeGo {
    async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        NodeGo::start(self, toxiproxy).await
    }

    async fn stop(&mut self) -> Result<()> {
        NodeGo::stop(self).await
    }

    fn is_running(&self) -> bool {
        NodeGo::is_running(self)
    }

    fn url(&self) -> Url {
        self.grpc_url()
    }

    fn external_url(&self) -> Url {
        self.external_grpc_url().unwrap_or_else(|| self.grpc_url())
    }

    fn name(&self) -> String {
        "node_go".to_string()
    }

    fn port(&self) -> u16 {
        NodeGoConst::API_PORT
    }
}
