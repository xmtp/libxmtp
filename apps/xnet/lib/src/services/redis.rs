//! Redis container management for D14n.

use async_trait::async_trait;
use bollard::{
    Docker,
    models::{ContainerCreateBody, HealthConfig, HostConfig},
    query_parameters::CreateContainerOptionsBuilder,
};
use bon::Builder;
use color_eyre::eyre::Result;
use url::Url;

use crate::{
    constants::Redis as RedisConst,
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy},
};

/// Manages a Redis Docker container for D14n.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
pub struct Redis {
    /// The Redis image
    #[builder(default = RedisConst::IMAGE.to_string())]
    image: String,

    /// Managed container state
    #[builder(default = ManagedContainer::new())]
    container: ManagedContainer,
}

impl Redis {
    /// Start the Redis container.
    ///
    /// Registers itself with ToxiProxy for external access.
    /// If a container with the same name already exists, it will be reused.
    pub async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        let options = CreateContainerOptionsBuilder::default().name(RedisConst::CONTAINER_NAME);

        let config = ContainerCreateBody {
            image: Some(self.image.clone()),
            healthcheck: Some(HealthConfig {
                test: Some(vec![
                    "CMD".to_string(),
                    "redis-cli".to_string(),
                    "ping".to_string(),
                ]),
                interval: Some(10_000_000_000),
                timeout: Some(5_000_000_000),
                retries: Some(3),
                start_period: Some(5_000_000_000),
                start_interval: None,
            }),
            host_config: Some(HostConfig {
                network_mode: Some(XNET_NETWORK_NAME.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        self.container
            .start_container(RedisConst::CONTAINER_NAME, options, config)
            .await?;

        // Register with ToxiProxy
        let port = self.register(toxiproxy, None).await?;
        self.container.set_proxy_port(port);

        Ok(())
    }

    /// Stop the Redis container.
    pub async fn stop(&mut self) -> Result<()> {
        self.container
            .stop_container(RedisConst::CONTAINER_NAME)
            .await
    }

    /// Redis URL for use within the docker network.
    pub fn url(&self) -> Url {
        Url::parse(&format!(
            "redis://{}:{}",
            RedisConst::CONTAINER_NAME,
            RedisConst::PORT
        ))
        .expect("valid URL")
    }

    /// Redis URL for external access (through ToxiProxy).
    pub fn external_url(&self) -> Option<Url> {
        self.container
            .proxy_port()
            .map(|port| Url::parse(&format!("redis://localhost:{}", port)).expect("valid URL"))
    }

    /// Get the ToxiProxy port for this service.
    pub fn proxy_port(&self) -> Option<u16> {
        self.container.proxy_port()
    }

    /// Check if Redis is running.
    pub fn is_running(&self) -> bool {
        self.container.is_running()
    }
}

#[async_trait]
impl Service for Redis {
    async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        Redis::start(self, toxiproxy).await
    }

    async fn stop(&mut self) -> Result<()> {
        Redis::stop(self).await
    }

    fn is_running(&self) -> bool {
        Redis::is_running(self)
    }

    fn url(&self) -> Url {
        Redis::url(self)
    }

    fn external_url(&self) -> Url {
        self.external_url().unwrap_or_else(|| self.url())
    }

    fn name(&self) -> String {
        "redis".to_string()
    }

    fn port(&self) -> u16 {
        RedisConst::PORT
    }
}
