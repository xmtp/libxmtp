//! XMTPD Gateway container management.
//!
//! The gateway provides the API layer for XMTP clients, connecting to xmtpd
//! and Redis for caching.

use std::default;

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
    constants::{Anvil as AnvilConst, Gateway as GatewayConst, Redis as RedisConst},
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy},
};

/// Manages an XMTPD Gateway Docker container.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
#[builder(finish_fn(vis = "", name = build_internal))]
pub struct Gateway {
    /// The image name (e.g., "ghcr.io/xmtp/xmtpd-gateway")
    #[builder(default = GatewayConst::IMAGE.to_string())]
    image: String,

    /// The version tag for the gateway image (e.g., "main", "v1.0.0")
    #[builder(default = GatewayConst::VERSION.to_string())]
    version: String,

    /// API port for the gateway (inside the container)
    #[builder(default = GatewayConst::PORT)]
    api_port: u16,

    /// Redis URL for caching
    #[builder(default = default_redis_host())]
    redis_host: String,

    /// Anvil URL to chain
    #[builder(default = default_anvil_host())]
    anvil_host: String,

    #[builder(default = "anvil".to_string())]
    contracts_environment: String,

    /// Log level
    #[builder(default = "debug".to_string())]
    log_level: String,

    /// Enable gRPC reflection
    #[builder(default = true)]
    reflection_enable: bool,

    /// Managed container state
    #[builder(default = ManagedContainer::new())]
    container: ManagedContainer,
}

impl<S: gateway_builder::IsComplete> GatewayBuilder<S> {
    pub fn build(self) -> Result<Gateway> {
        // Delegate to `build_internal()` to get the instance of user.
        let mut this = self.build_internal();
        let config = Config::load()?;
        if let Some(version) = config.gateway.version {
            this.version = version;
        }
        if let Some(image) = config.gateway.image {
            this.image = image;
        }
        Ok(this)
    }
}

fn default_redis_host() -> String {
    format!("{}:{}", RedisConst::CONTAINER_NAME, RedisConst::PORT)
}

fn default_anvil_host() -> String {
    format!("{}:{}", AnvilConst::CONTAINER_NAME, AnvilConst::PORT)
}

impl Gateway {
    /// Start the gateway container.
    ///
    /// Registers itself with ToxiProxy for external access.
    /// If a container with the same name already exists, it will be reused.
    pub async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        let options = CreateContainerOptionsBuilder::default()
            .name(GatewayConst::CONTAINER_NAME)
            .platform("linux/amd64");

        let Gateway {
            contracts_environment,
            redis_host,
            anvil_host,
            image,
            version,
            api_port,
            log_level,
            reflection_enable,
            ..
        } = self;

        // port is hardcoded to 5050 then proxy forwards 5052 -> 5050
        let redis_url = format!("redis://{redis_host}/0");
        let env = vec![
            format!("XMTPD_CONTRACTS_ENVIRONMENT={contracts_environment}"),
            format!("XMTPD_API_PORT=5050"),
            format!("XMTPD_REDIS_URL={redis_url}"),
            format!("XMTPD_LOG_LEVEL={log_level}"),
            format!("XMTPD_REFLECTION_ENABLE={reflection_enable}"),
            format!("XMTPD_PAYER_PRIVATE_KEY={}", GatewayConst::PRIVATE_KEY),
            format!("XMTPD_APP_CHAIN_RPC_URL=http://{anvil_host}"),
            format!("XMTPD_APP_CHAIN_WSS_URL=ws://{anvil_host}"),
            format!("XMTPD_SETTLEMENT_CHAIN_RPC_URL=http://{anvil_host}"),
            format!("XMTPD_SETTLEMENT_CHAIN_WSS_URL=ws://{anvil_host}"),
        ];

        let config = ContainerCreateBody {
            image: Some(format!("{image}:{version}")),
            env: Some(env),
            host_config: Some(HostConfig {
                network_mode: Some(XNET_NETWORK_NAME.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        self.container
            .start_container(GatewayConst::CONTAINER_NAME, options, config)
            .await?;

        let port = self.register(toxiproxy, Some(GatewayConst::PORT)).await?;
        self.container.set_proxy_port(port);

        Ok(())
    }

    /// Stop the gateway container.
    pub async fn stop(&mut self) -> Result<()> {
        self.container
            .stop_container(GatewayConst::CONTAINER_NAME)
            .await
    }

    /// Gateway URL for use within the docker network.
    pub fn url(&self) -> Url {
        Url::parse(&format!("http://{}:5050", GatewayConst::CONTAINER_NAME)).expect("valid URL")
    }

    /// Gateway URL for external access (through ToxiProxy).
    pub fn external_url(&self) -> Option<Url> {
        info!("proxy port {:?}", self.container.proxy_port());
        info!("port {}", self.port());
        self.container
            .proxy_port()
            .map(|port| Url::parse(&format!("http://localhost:{}", port)).expect("valid URL"))
    }

    /// Get the ToxiProxy port for this service.
    pub fn proxy_port(&self) -> Option<u16> {
        self.container.proxy_port()
    }

    /// Check if gateway is running.
    pub fn is_running(&self) -> bool {
        self.container.is_running()
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

    fn url(&self) -> Url {
        Gateway::url(self)
    }

    fn external_url(&self) -> Url {
        self.external_url().expect("external url must be correct")
    }

    fn name(&self) -> String {
        "gateway".to_string()
    }

    fn port(&self) -> u16 {
        GatewayConst::PORT
    }
}
