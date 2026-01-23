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
    config::{
        DEFAULT_NODE_GO_IMAGE, DEFAULT_NODE_GO_NODE_KEY, DEFAULT_NODE_GO_VERSION,
        DEFAULT_POSTGRES_PASSWORD, MLS_DB_CONTAINER_NAME, MLS_DB_PORT, NODE_GO_API_HTTP_PORT,
        NODE_GO_API_PORT, NODE_GO_CONTAINER_NAME, V3_DB_CONTAINER_NAME, V3_DB_PORT,
        VALIDATION_CONTAINER_NAME, VALIDATION_PORT,
    },
    network::XNET_NETWORK_NAME,
    services::{
        ContainerState, Service, ToxiProxy, create_and_start_container, ensure_container_running,
        stop_and_remove_container,
    },
};

/// Manages a Node-Go (XMTP node) Docker container.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
pub struct NodeGo {
    /// The version tag for the node-go image (e.g., "main", "v1.0.0")
    #[builder(default = DEFAULT_NODE_GO_VERSION.to_string())]
    version: String,

    /// Node key for the node
    #[builder(default = DEFAULT_NODE_GO_NODE_KEY.to_string())]
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

    /// Wait for database timeout
    #[builder(default = "30s".to_string())]
    wait_for_db: String,

    /// Docker client (initialized on start)
    #[builder(skip)]
    docker: Option<Docker>,

    /// Container ID once started
    #[builder(skip)]
    container_id: Option<String>,

    /// ToxiProxy port for gRPC API (set after registering with ToxiProxy)
    #[builder(skip)]
    grpc_proxy_port: Option<u16>,

    /// ToxiProxy port for HTTP API (set after registering with ToxiProxy)
    #[builder(skip)]
    http_proxy_port: Option<u16>,
}

fn default_store_db_host() -> String {
    format!("{}:{}", V3_DB_CONTAINER_NAME, V3_DB_PORT)
}

fn default_mls_store_db_connection_string() -> String {
    format!("{}:{}", MLS_DB_CONTAINER_NAME, MLS_DB_PORT)
}

fn default_mls_validation_address() -> String {
    format!("{}:{}", VALIDATION_CONTAINER_NAME, VALIDATION_PORT)
}

impl NodeGo {
    /// Start the node-go container.
    ///
    /// Registers itself with ToxiProxy for external access (both gRPC and HTTP).
    /// If a container with the same name already exists, it will be reused.
    pub async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        let docker = Docker::connect_with_socket_defaults()?;

        let container_id = match ensure_container_running(&docker, NODE_GO_CONTAINER_NAME).await? {
            ContainerState::Exists(id) => id,
            ContainerState::NotFound => {
                let NodeGo {
                    store_db_host,
                    mls_store_db_host,
                    mls_validation_address,
                    wait_for_db,
                    node_key,
                    ..
                } = &self;

                let options = CreateContainerOptionsBuilder::default()
                    .name(NODE_GO_CONTAINER_NAME)
                    .platform("linux/amd64");

                let db_connection_str =
                    super::db_connection_string(DEFAULT_POSTGRES_PASSWORD, store_db_host);
                let mls_db_connection_str =
                    super::db_connection_string(DEFAULT_POSTGRES_PASSWORD, mls_store_db_host);
                let cmd = vec![
                    "--store.enable".to_string(),
                    format!("--store.db-connection-string={db_connection_str}",),
                    format!("--store.reader-db-connection-string={db_connection_str}",),
                    format!("--mls-store.db-connection-string={mls_db_connection_str}",),
                    format!("--mls-validation.grpc-address={mls_validation_address}",),
                    "--api.enable-mls".to_string(),
                    format!("--wait-for-db={wait_for_db}"),
                ];

                let image = format!("{}:{}", DEFAULT_NODE_GO_IMAGE, self.version);
                let config = ContainerCreateBody {
                    image: Some(image),
                    cmd: Some(cmd),
                    env: Some(vec![format!("GOWAKU-NODEKEY={node_key}")]),
                    host_config: Some(HostConfig {
                        network_mode: Some(XNET_NETWORK_NAME.to_string()),
                        ..Default::default()
                    }),
                    ..Default::default()
                };

                create_and_start_container(&docker, NODE_GO_CONTAINER_NAME, options, config).await?
            }
        };

        self.docker = Some(docker);
        self.container_id = Some(container_id);

        // Register gRPC API with ToxiProxy
        let grpc_upstream = format!("{}:{}", NODE_GO_CONTAINER_NAME, NODE_GO_API_PORT);
        let grpc_port = toxiproxy.register("node_go_grpc", grpc_upstream).await?;
        self.grpc_proxy_port = Some(grpc_port);

        // Register HTTP API with ToxiProxy
        let http_upstream = format!("{}:{}", NODE_GO_CONTAINER_NAME, NODE_GO_API_HTTP_PORT);
        let http_port = toxiproxy.register("node_go_http", http_upstream).await?;
        self.http_proxy_port = Some(http_port);

        Ok(())
    }

    /// Stop and remove the node-go container.
    pub async fn stop(&mut self) -> Result<()> {
        if let (Some(docker), Some(id)) = (&self.docker, &self.container_id) {
            stop_and_remove_container(docker, id, NODE_GO_CONTAINER_NAME).await?;
        }
        self.container_id = None;
        self.grpc_proxy_port = None;
        self.http_proxy_port = None;
        Ok(())
    }

    /// gRPC API URL for use within the docker network.
    pub fn grpc_url(&self) -> Url {
        Url::parse(&format!(
            "http://{}:{}",
            NODE_GO_CONTAINER_NAME, NODE_GO_API_PORT
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
            NODE_GO_CONTAINER_NAME, NODE_GO_API_HTTP_PORT
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
        self.container_id.is_some()
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

    fn name(&self) -> &'static str {
        "node_go"
    }
}
