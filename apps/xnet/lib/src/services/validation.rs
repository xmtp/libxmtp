//! MLS Validation Service container management.

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
    constants::{Anvil as AnvilConst, Validation as ValidationConst},
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy},
};

fn default_anvil_url() -> Url {
    Url::parse(&format!(
        "http://{}:{}",
        AnvilConst::CONTAINER_NAME,
        AnvilConst::PORT
    ))
    .expect("valid URL")
}

/// Manages an MLS Validation Service Docker container.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
#[builder(finish_fn(vis = "", name = build_internal))]
pub struct Validation {
    /// The image name (e.g., "ghcr.io/xmtp/mls-validation-service")
    #[builder(default = ValidationConst::IMAGE.to_string())]
    image: String,

    /// The version tag for the validation service image (e.g., "main", "v1.0.0")
    #[builder(default = ValidationConst::VERSION.to_string())]
    version: String,

    /// Anvil URL for the validation service to connect to
    #[builder(default = default_anvil_url())]
    anvil_url: Url,

    /// Managed container state
    #[builder(default = ManagedContainer::new())]
    container: ManagedContainer,
}

impl<S: validation_builder::IsComplete> ValidationBuilder<S> {
    pub fn build(self) -> Result<Validation> {
        // Delegate to `build_internal()` to get the instance of user.
        let mut this = self.build_internal();
        let config = Config::load()?;
        if let Some(version) = config.validation.version {
            this.version = version;
        }
        if let Some(image) = config.validation.image {
            this.image = image;
        }
        Ok(this)
    }
}

impl Validation {
    /// Start the validation service container.
    ///
    /// Registers itself with ToxiProxy for external access.
    /// If a container with the same name already exists, it will be reused.
    pub async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        let options =
            CreateContainerOptionsBuilder::default().name(ValidationConst::CONTAINER_NAME);

        let image = format!("{}:{}", self.image, self.version);
        let config = ContainerCreateBody {
            image: Some(image),
            env: Some(vec![format!("ANVIL_URL={}", self.anvil_url)]),
            host_config: Some(HostConfig {
                network_mode: Some(XNET_NETWORK_NAME.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        self.container
            .start_container(ValidationConst::CONTAINER_NAME, options, config)
            .await?;

        let port = self.register(toxiproxy, None).await?;
        self.container.set_proxy_port(port);

        Ok(())
    }

    /// Stop the validation service container.
    pub async fn stop(&mut self) -> Result<()> {
        self.container
            .stop_container(ValidationConst::CONTAINER_NAME)
            .await
    }

    /// Validation service gRPC address for use within the docker network.
    pub fn grpc_address(&self) -> String {
        format!(
            "{}:{}",
            ValidationConst::CONTAINER_NAME,
            ValidationConst::PORT
        )
    }

    /// Validation service gRPC address for external access (through ToxiProxy).
    pub fn external_grpc_address(&self) -> Option<String> {
        self.container
            .proxy_port()
            .map(|port| format!("localhost:{}", port))
    }

    /// Get the ToxiProxy port for this service.
    pub fn proxy_port(&self) -> Option<u16> {
        self.container.proxy_port()
    }

    /// Check if validation service is running.
    pub fn is_running(&self) -> bool {
        self.container.is_running()
    }
}

#[async_trait]
impl Service for Validation {
    async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        Validation::start(self, toxiproxy).await
    }

    async fn stop(&mut self) -> Result<()> {
        Validation::stop(self).await
    }

    fn is_running(&self) -> bool {
        Validation::is_running(self)
    }

    fn url(&self) -> Url {
        Url::parse(&format!("http://{}", self.grpc_address())).expect("valid URL")
    }

    fn external_url(&self) -> Url {
        let address = self
            .external_grpc_address()
            .unwrap_or_else(|| self.grpc_address());
        Url::parse(&format!("http://{}", address)).expect("valid URL")
    }

    fn name(&self) -> String {
        "validation".to_string()
    }

    fn port(&self) -> u16 {
        ValidationConst::PORT
    }
}
