//! XMTPD Gateway container management.
//!
//! The gateway provides the API layer for XMTP clients, connecting to xmtpd
//! and Redis for caching.

use async_trait::async_trait;
use bollard::{
    Docker,
    models::{ContainerCreateBody, HostConfig},
    query_parameters::CreateContainerOptionsBuilder,
};
use bon::Builder;
use color_eyre::eyre::Result;

use crate::{
    config::{
        DEFAULT_GATEWAY_IMAGE, DEFAULT_GATEWAY_VERSION, GATEWAY_CONTAINER_NAME, GATEWAY_PORT,
        REDIS_CONTAINER_NAME, REDIS_PORT,
    },
    network::XNET_NETWORK_NAME,
    services::{
        ContainerState, Service, ToxiProxy, create_and_start_container, ensure_container_running,
        stop_and_remove_container,
    },
};

/// Manages an XMTPD Gateway Docker container.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
pub struct Gateway {
    /// The version tag for the gateway image (e.g., "main", "v1.0.0")
    #[builder(default = DEFAULT_GATEWAY_VERSION.to_string())]
    version: String,

    /// API port for the gateway (inside the container)
    #[builder(default = GATEWAY_PORT)]
    api_port: u16,

    /// Redis URL for caching
    #[builder(default = default_redis_url())]
    redis_url: String,

    /// Path to the contracts config file inside the container
    #[builder(default = "/cfg/anvil.json".to_string())]
    contracts_config_path: String,

    /// Log level
    #[builder(default = "debug".to_string())]
    log_level: String,

    /// Enable gRPC reflection
    #[builder(default = true)]
    reflection_enable: bool,

    /// Docker client (initialized on start)
    #[builder(skip)]
    docker: Option<Docker>,

    /// Container ID once started
    #[builder(skip)]
    container_id: Option<String>,

    /// ToxiProxy port for external access (set after registering with ToxiProxy)
    #[builder(skip)]
    proxy_port: Option<u16>,
}

fn default_redis_url() -> String {
    format!("redis://{}:{}/0", REDIS_CONTAINER_NAME, REDIS_PORT)
}

impl Gateway {
    /// Start the gateway container.
    ///
    /// Registers itself with ToxiProxy for external access.
    /// If a container with the same name already exists, it will be reused.
    pub async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        let docker = Docker::connect_with_socket_defaults()?;

        let container_id = match ensure_container_running(&docker, GATEWAY_CONTAINER_NAME).await? {
            ContainerState::Exists(id) => id,
            ContainerState::NotFound => {
                let options = CreateContainerOptionsBuilder::default()
                    .name(GATEWAY_CONTAINER_NAME)
                    .platform("linux/amd64");

                let env = vec![
                    format!(
                        "XMTPD_CONTRACTS_CONFIG_FILE_PATH={}",
                        self.contracts_config_path
                    ),
                    format!("XMTPD_API_PORT={}", self.api_port),
                    format!("XMTPD_REDIS_URL={}", self.redis_url),
                    format!("XMTPD_LOG_LEVEL={}", self.log_level),
                    format!("XMTPD_REFLECTION_ENABLE={}", self.reflection_enable),
                ];

                let image = format!("{}:{}", DEFAULT_GATEWAY_IMAGE, self.version);
                let config = ContainerCreateBody {
                    image: Some(image),
                    env: Some(env),
                    host_config: Some(HostConfig {
                        network_mode: Some(XNET_NETWORK_NAME.to_string()),
                        ..Default::default()
                    }),
                    ..Default::default()
                };

                create_and_start_container(&docker, GATEWAY_CONTAINER_NAME, options, config).await?
            }
        };

        self.docker = Some(docker);
        self.container_id = Some(container_id);

        // Register with ToxiProxy
        let upstream = format!("{}:{}", GATEWAY_CONTAINER_NAME, self.api_port);
        let port = toxiproxy.register("gateway", upstream).await?;
        self.proxy_port = Some(port);

        Ok(())
    }

    /// Stop and remove the gateway container.
    pub async fn stop(&mut self) -> Result<()> {
        if let (Some(docker), Some(id)) = (&self.docker, &self.container_id) {
            stop_and_remove_container(docker, id, GATEWAY_CONTAINER_NAME).await?;
        }
        self.container_id = None;
        self.proxy_port = None;
        Ok(())
    }

    /// Gateway URL for use within the docker network.
    pub fn url(&self) -> String {
        format!("http://{}:{}", GATEWAY_CONTAINER_NAME, self.api_port)
    }

    /// Gateway URL for external access (through ToxiProxy).
    pub fn external_url(&self) -> Option<String> {
        self.proxy_port
            .map(|port| format!("http://localhost:{}", port))
    }

    /// Get the ToxiProxy port for this service.
    pub fn proxy_port(&self) -> Option<u16> {
        self.proxy_port
    }

    /// Check if gateway is running.
    pub fn is_running(&self) -> bool {
        self.container_id.is_some()
    }
}

#[async_trait]
impl Service for Gateway {
    async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        Gateway::start(self, toxiproxy).await
    }

    async fn stop(&mut self) -> Result<()> {
        Gateway::stop(self).await
    }

    fn is_running(&self) -> bool {
        Gateway::is_running(self)
    }

    fn url(&self) -> String {
        Gateway::url(self)
    }

    fn external_url(&self) -> String {
        self.external_url().unwrap_or_else(|| self.url())
    }

    fn name(&self) -> &'static str {
        "gateway"
    }
}
