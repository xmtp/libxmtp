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
    Config,
    constants::HistoryServer as HistoryServerConst,
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy},
};

/// Manages a Message History Server Docker container.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
#[builder(finish_fn(vis = "", name = build_internal))]
pub struct HistoryServer {
    /// The image name (e.g., "ghcr.io/xmtp/message-history-server")
    #[builder(default = HistoryServerConst::IMAGE.to_string())]
    image: String,

    /// The version tag for the history server image (e.g., "main", "v1.0.0")
    #[builder(default = HistoryServerConst::VERSION.to_string())]
    version: String,

    /// Managed container state
    #[builder(default = ManagedContainer::new())]
    container: ManagedContainer,
}

impl<S: history_server_builder::IsComplete> HistoryServerBuilder<S> {
    pub fn build(self) -> Result<HistoryServer> {
        // Delegate to `build_internal()` to get the instance of user.
        let mut this = self.build_internal();
        let config = Config::load()?;
        if let Some(version) = config.history.version {
            this.version = version;
        }
        if let Some(image) = config.history.image {
            this.image = image;
        }
        Ok(this)
    }
}

impl HistoryServer {
    /// Start the history server container.
    ///
    /// Registers itself with ToxiProxy for external access.
    /// If a container with the same name already exists, it will be reused.
    pub async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        let options = CreateContainerOptionsBuilder::default()
            .name(HistoryServerConst::CONTAINER_NAME)
            .platform("linux/amd64");

        let image = format!("{}:{}", self.image, self.version);
        let config = ContainerCreateBody {
            image: Some(image),
            host_config: Some(HostConfig {
                network_mode: Some(XNET_NETWORK_NAME.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        self.container
            .start_container(HistoryServerConst::CONTAINER_NAME, options, config)
            .await?;

        let port = self.register(toxiproxy, None).await?;
        self.container.set_proxy_port(port);

        Ok(())
    }

    /// Stop the history server container.
    pub async fn stop(&mut self) -> Result<()> {
        self.container
            .stop_container(HistoryServerConst::CONTAINER_NAME)
            .await
    }

    /// History server URL for use within the docker network.
    pub fn url(&self) -> Url {
        Url::parse(&format!(
            "http://{}:{}",
            HistoryServerConst::CONTAINER_NAME,
            HistoryServerConst::PORT
        ))
        .expect("valid URL")
    }

    /// History server URL for external access (through ToxiProxy).
    pub fn external_url(&self) -> Option<Url> {
        self.container
            .proxy_port()
            .map(|port| Url::parse(&format!("http://localhost:{}", port)).expect("valid URL"))
    }

    /// Get the ToxiProxy port for this service.
    pub fn proxy_port(&self) -> Option<u16> {
        self.container.proxy_port()
    }

    /// Check if history server is running.
    pub fn is_running(&self) -> bool {
        self.container.is_running()
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

    fn name(&self) -> String {
        "history_server".to_string()
    }

    fn port(&self) -> u16 {
        HistoryServerConst::PORT
    }
}
