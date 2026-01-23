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
    config::{
        ANVIL_CONTAINER_NAME, ANVIL_PORT, DEFAULT_VALIDATION_IMAGE, DEFAULT_VALIDATION_VERSION,
        VALIDATION_CONTAINER_NAME, VALIDATION_PORT,
    },
    network::XNET_NETWORK_NAME,
    services::{
        ContainerState, Service, ToxiProxy, create_and_start_container, ensure_container_running,
        stop_and_remove_container,
    },
};

fn default_anvil_url() -> Url {
    Url::parse(&format!("http://{}:{}", ANVIL_CONTAINER_NAME, ANVIL_PORT))
        .expect("valid URL")
}

/// Manages an MLS Validation Service Docker container.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
pub struct Validation {
    /// The version tag for the validation service image (e.g., "main", "v1.0.0")
    #[builder(default = DEFAULT_VALIDATION_VERSION.to_string())]
    version: String,

    /// Anvil URL for the validation service to connect to
    #[builder(default = default_anvil_url())]
    anvil_url: Url,

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

impl Validation {
    /// Start the validation service container.
    ///
    /// Registers itself with ToxiProxy for external access.
    /// If a container with the same name already exists, it will be reused.
    pub async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        let docker = Docker::connect_with_socket_defaults()?;

        let container_id =
            match ensure_container_running(&docker, VALIDATION_CONTAINER_NAME).await? {
                ContainerState::Exists(id) => id,
                ContainerState::NotFound => {
                    let options = CreateContainerOptionsBuilder::default()
                        .name(VALIDATION_CONTAINER_NAME)
                        .platform("linux/amd64");

                    let image = format!("{}:{}", DEFAULT_VALIDATION_IMAGE, self.version);
                    let config = ContainerCreateBody {
                        image: Some(image),
                        env: Some(vec![format!("ANVIL_URL={}", self.anvil_url)]),
                        host_config: Some(HostConfig {
                            network_mode: Some(XNET_NETWORK_NAME.to_string()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    };

                    create_and_start_container(&docker, VALIDATION_CONTAINER_NAME, options, config)
                        .await?
                }
            };

        self.docker = Some(docker);
        self.container_id = Some(container_id);

        // Register with ToxiProxy
        let upstream = format!("{}:{}", VALIDATION_CONTAINER_NAME, VALIDATION_PORT);
        let port = toxiproxy.register("validation", upstream).await?;
        self.proxy_port = Some(port);

        Ok(())
    }

    /// Stop and remove the validation service container.
    pub async fn stop(&mut self) -> Result<()> {
        if let (Some(docker), Some(id)) = (&self.docker, &self.container_id) {
            stop_and_remove_container(docker, id, VALIDATION_CONTAINER_NAME).await?;
        }
        self.container_id = None;
        self.proxy_port = None;
        Ok(())
    }

    /// Validation service gRPC address for use within the docker network.
    pub fn grpc_address(&self) -> String {
        format!("{}:{}", VALIDATION_CONTAINER_NAME, VALIDATION_PORT)
    }

    /// Validation service gRPC address for external access (through ToxiProxy).
    pub fn external_grpc_address(&self) -> Option<String> {
        self.proxy_port
            .map(|port| format!("localhost:{}", port))
    }

    /// Get the ToxiProxy port for this service.
    pub fn proxy_port(&self) -> Option<u16> {
        self.proxy_port
    }

    /// Check if validation service is running.
    pub fn is_running(&self) -> bool {
        self.container_id.is_some()
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

    fn name(&self) -> &'static str {
        "validation"
    }
}
