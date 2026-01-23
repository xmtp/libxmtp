//! Message History Server container management.

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
        DEFAULT_HISTORY_SERVER_IMAGE, DEFAULT_HISTORY_SERVER_VERSION, HISTORY_SERVER_CONTAINER_NAME,
        HISTORY_SERVER_PORT,
    },
    network::XNET_NETWORK_NAME,
    services::{
        ContainerState, Service, ToxiProxy, create_and_start_container, ensure_container_running,
        stop_and_remove_container,
    },
};

/// Manages a Message History Server Docker container.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
pub struct HistoryServer {
    /// The version tag for the history server image (e.g., "main", "v1.0.0")
    #[builder(default = DEFAULT_HISTORY_SERVER_VERSION.to_string())]
    version: String,

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

impl HistoryServer {
    /// Start the history server container.
    ///
    /// Registers itself with ToxiProxy for external access.
    /// If a container with the same name already exists, it will be reused.
    pub async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        let docker = Docker::connect_with_socket_defaults()?;

        let container_id = match ensure_container_running(&docker, HISTORY_SERVER_CONTAINER_NAME)
            .await?
        {
            ContainerState::Exists(id) => id,
            ContainerState::NotFound => {
                let options = CreateContainerOptionsBuilder::default()
                    .name(HISTORY_SERVER_CONTAINER_NAME)
                    .platform("linux/amd64");

                let image = format!("{}:{}", DEFAULT_HISTORY_SERVER_IMAGE, self.version);
                let config = ContainerCreateBody {
                    image: Some(image),
                    host_config: Some(HostConfig {
                        network_mode: Some(XNET_NETWORK_NAME.to_string()),
                        ..Default::default()
                    }),
                    ..Default::default()
                };

                create_and_start_container(&docker, HISTORY_SERVER_CONTAINER_NAME, options, config)
                    .await?
            }
        };

        self.docker = Some(docker);
        self.container_id = Some(container_id);

        // Register with ToxiProxy
        let upstream = format!("{}:{}", HISTORY_SERVER_CONTAINER_NAME, HISTORY_SERVER_PORT);
        let port = toxiproxy.register("history_server", upstream).await?;
        self.proxy_port = Some(port);

        Ok(())
    }

    /// Stop and remove the history server container.
    pub async fn stop(&mut self) -> Result<()> {
        if let (Some(docker), Some(id)) = (&self.docker, &self.container_id) {
            stop_and_remove_container(docker, id, HISTORY_SERVER_CONTAINER_NAME).await?;
        }
        self.container_id = None;
        self.proxy_port = None;
        Ok(())
    }

    /// History server URL for use within the docker network.
    pub fn url(&self) -> Url {
        Url::parse(&format!(
            "http://{}:{}",
            HISTORY_SERVER_CONTAINER_NAME, HISTORY_SERVER_PORT
        ))
        .expect("valid URL")
    }

    /// History server URL for external access (through ToxiProxy).
    pub fn external_url(&self) -> Option<Url> {
        self.proxy_port.map(|port| {
            Url::parse(&format!("http://localhost:{}", port)).expect("valid URL")
        })
    }

    /// Get the ToxiProxy port for this service.
    pub fn proxy_port(&self) -> Option<u16> {
        self.proxy_port
    }

    /// Check if history server is running.
    pub fn is_running(&self) -> bool {
        self.container_id.is_some()
    }
}

#[async_trait]
impl Service for HistoryServer {
    async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        HistoryServer::start(self, toxiproxy).await
    }

    async fn stop(&mut self) -> Result<()> {
        HistoryServer::stop(self).await
    }

    fn is_running(&self) -> bool {
        HistoryServer::is_running(self)
    }

    fn url(&self) -> Url {
        HistoryServer::url(self)
    }

    fn external_url(&self) -> Url {
        self.external_url().unwrap_or_else(|| self.url())
    }

    fn name(&self) -> &'static str {
        "history_server"
    }
}
