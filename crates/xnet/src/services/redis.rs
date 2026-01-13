//! Redis container management for D14n.

use async_trait::async_trait;
use bollard::{
    Docker,
    models::{ContainerCreateBody, HealthConfig, HostConfig},
    query_parameters::CreateContainerOptionsBuilder,
};
use bon::Builder;
use color_eyre::eyre::Result;

use crate::{
    config::{DEFAULT_REDIS_IMAGE, REDIS_CONTAINER_NAME, REDIS_PORT},
    network::XNET_NETWORK_NAME,
    services::{
        ContainerState, Service, ToxiProxy, create_and_start_container, ensure_container_running,
        stop_and_remove_container,
    },
};

/// Manages a Redis Docker container for D14n.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
pub struct Redis {
    /// The Redis image
    #[builder(default = DEFAULT_REDIS_IMAGE.to_string())]
    image: String,

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

impl Redis {
    /// Start the Redis container.
    ///
    /// Registers itself with ToxiProxy for external access.
    /// If a container with the same name already exists, it will be reused.
    pub async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        let docker = Docker::connect_with_socket_defaults()?;

        let container_id = match ensure_container_running(&docker, REDIS_CONTAINER_NAME).await? {
            ContainerState::Exists(id) => id,
            ContainerState::NotFound => {
                let options = CreateContainerOptionsBuilder::default().name(REDIS_CONTAINER_NAME);

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

                create_and_start_container(&docker, REDIS_CONTAINER_NAME, options, config).await?
            }
        };

        self.docker = Some(docker);
        self.container_id = Some(container_id);

        // Register with ToxiProxy
        let upstream = format!("{}:{}", REDIS_CONTAINER_NAME, REDIS_PORT);
        let port = toxiproxy.register("redis", upstream).await?;
        self.proxy_port = Some(port);

        Ok(())
    }

    /// Stop and remove the Redis container.
    pub async fn stop(&mut self) -> Result<()> {
        if let (Some(docker), Some(id)) = (&self.docker, &self.container_id) {
            stop_and_remove_container(docker, id, REDIS_CONTAINER_NAME).await?;
        }
        self.container_id = None;
        self.proxy_port = None;
        Ok(())
    }

    /// Redis URL for use within the docker network.
    pub fn url(&self) -> String {
        format!("redis://{}:{}", REDIS_CONTAINER_NAME, REDIS_PORT)
    }

    /// Redis URL for external access (through ToxiProxy).
    pub fn external_url(&self) -> Option<String> {
        self.proxy_port
            .map(|port| format!("redis://localhost:{}", port))
    }

    /// Get the ToxiProxy port for this service.
    pub fn proxy_port(&self) -> Option<u16> {
        self.proxy_port
    }

    /// Check if Redis is running.
    pub fn is_running(&self) -> bool {
        self.container_id.is_some()
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

    fn url(&self) -> String {
        Redis::url(self)
    }

    fn external_url(&self) -> String {
        self.external_url().unwrap_or_else(|| self.url())
    }

    fn name(&self) -> &'static str {
        "redis"
    }
}
