//! Anvil chain container management using the ghcr.io/xmtp/contracts image.
//!
//! This image starts Anvil AND deploys XMTP contracts automatically.

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
    constants::Anvil as AnvilConst,
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy},
};

/// Manages an Anvil chain Docker container with XMTP contracts pre-deployed.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
#[builder(finish_fn(vis = "", name = build_internal))]
pub struct Anvil {
    /// The image name (e.g., "ghcr.io/xmtp/contracts")
    #[builder(default = AnvilConst::IMAGE.to_string())]
    image: String,

    /// The version tag for the contracts image (e.g., "v0.5.5", "main")
    #[builder(default = AnvilConst::VERSION.to_string())]
    version: String,

    /// Managed container state
    #[builder(default = ManagedContainer::new())]
    container: ManagedContainer,
}

impl<S: anvil_builder::IsComplete> AnvilBuilder<S> {
    pub fn build(self) -> Result<Anvil> {
        // Delegate to `build_internal()` to get the instance of user.
        let mut this = self.build_internal();
        let config = Config::load()?;
        if let Some(version) = config.contracts.version {
            this.version = version;
        }
        if let Some(image) = config.contracts.image {
            this.image = image;
        }
        Ok(this)
    }
}

impl Anvil {
    /// Start the Anvil container with XMTP contracts deployed.
    ///
    /// Registers itself with ToxiProxy for external access.
    /// If a container with the same name already exists, it will be reused.
    pub async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        let options = CreateContainerOptionsBuilder::default()
            .name(AnvilConst::CONTAINER_NAME)
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
            .start_container(AnvilConst::CONTAINER_NAME, options, config)
            .await?;

        let port = self.register(toxiproxy, None).await?;
        self.container.set_proxy_port(port);

        Ok(())
    }

    /// Stop the Anvil container.
    pub async fn stop(&mut self) -> Result<()> {
        self.container
            .stop_container(AnvilConst::CONTAINER_NAME)
            .await
    }

    /// RPC URL for use within the docker network.
    pub fn rpc_url(&self) -> Url {
        Url::parse(&format!(
            "http://{}:{}",
            AnvilConst::CONTAINER_NAME,
            AnvilConst::PORT
        ))
        .expect("valid URL")
    }

    /// RPC URL for external access (through ToxiProxy).
    pub fn external_rpc_url(&self) -> Option<Url> {
        self.container
            .proxy_port()
            .map(|port| Url::parse(&format!("http://localhost:{}", port)).expect("valid URL"))
    }

    /// Get the ToxiProxy port for this service.
    pub fn proxy_port(&self) -> Option<u16> {
        self.container.proxy_port()
    }

    /// Check if anvil is running.
    pub fn is_running(&self) -> bool {
        self.container.is_running()
    }
}

#[async_trait]
impl Service for Anvil {
    async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        Anvil::start(self, toxiproxy).await
    }

    async fn stop(&mut self) -> Result<()> {
        Anvil::stop(self).await
    }

    fn is_running(&self) -> bool {
        Anvil::is_running(self)
    }

    fn url(&self) -> Url {
        self.rpc_url()
    }

    fn external_url(&self) -> Url {
        self.external_rpc_url().unwrap_or_else(|| self.rpc_url())
    }

    fn name(&self) -> String {
        "anvil".to_string()
    }

    fn port(&self) -> u16 {
        AnvilConst::PORT
    }
}
