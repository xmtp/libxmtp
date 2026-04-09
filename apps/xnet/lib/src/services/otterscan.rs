//! Otterscan block explorer UI container management.
//!
//! Runs the Otterscan Docker container connected to the Anvil instance.
//! Uses direct port binding (no ToxiProxy).

use std::collections::HashMap;

use async_trait::async_trait;
use bollard::{
    Docker,
    models::{ContainerCreateBody, HostConfig, PortBinding},
    query_parameters::CreateContainerOptionsBuilder,
};
use bon::Builder;
use color_eyre::eyre::Result;
use url::Url;

use crate::{
    constants::{Anvil as AnvilConst, Otterscan as OtterscanConst},
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy},
};

/// Manages an Otterscan block explorer Docker container.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
pub struct Otterscan {
    /// The image name (e.g., "otterscan/otterscan")
    #[builder(default = OtterscanConst::IMAGE.to_string())]
    image: String,

    /// The version tag (e.g., "latest")
    #[builder(default = OtterscanConst::VERSION.to_string())]
    version: String,

    /// The Anvil host for ERIGON_URL env var
    #[builder(default = format!("http://{}:{}", AnvilConst::CONTAINER_NAME, AnvilConst::PORT))]
    anvil_host: String,

    /// Managed container state
    #[builder(default = ManagedContainer::new())]
    container: ManagedContainer,
}

impl Otterscan {
    /// Start the Otterscan container.
    ///
    /// Uses direct port binding (5100:80) instead of ToxiProxy.
    pub async fn start(&mut self, _toxiproxy: &ToxiProxy) -> Result<()> {
        let Self {
            image,
            version,
            anvil_host,
            ..
        } = self;

        let options = CreateContainerOptionsBuilder::default().name(OtterscanConst::CONTAINER_NAME);

        let image_ref = format!("{image}:{version}");

        let mut port_bindings = HashMap::new();
        port_bindings.insert(
            format!("{}/tcp", OtterscanConst::PORT),
            Some(vec![PortBinding {
                host_ip: Some("0.0.0.0".to_string()),
                host_port: Some(OtterscanConst::EXTERNAL_PORT.to_string()),
            }]),
        );

        let config = ContainerCreateBody {
            image: Some(image_ref),
            env: Some(vec![format!("ERIGON_URL={anvil_host}")]),
            host_config: Some(HostConfig {
                network_mode: Some(XNET_NETWORK_NAME.to_string()),
                port_bindings: Some(port_bindings),
                ..Default::default()
            }),
            ..Default::default()
        };

        self.container
            .start_container(OtterscanConst::CONTAINER_NAME, options, config)
            .await?;

        Ok(())
    }

    /// Stop the Otterscan container.
    pub async fn stop(&mut self) -> Result<()> {
        self.container
            .stop_container(OtterscanConst::CONTAINER_NAME)
            .await
    }

    /// URL for use within the Docker network.
    pub fn url(&self) -> Url {
        Url::parse(&format!(
            "http://{}:{}",
            OtterscanConst::CONTAINER_NAME,
            OtterscanConst::PORT
        ))
        .expect("valid URL")
    }

    /// URL for external access (direct port binding).
    pub fn external_url(&self) -> Url {
        Url::parse(&format!(
            "http://localhost:{}",
            OtterscanConst::EXTERNAL_PORT
        ))
        .expect("valid URL")
    }

    /// Check if Otterscan is running.
    pub fn is_running(&self) -> bool {
        self.container.is_running()
    }
}

#[async_trait]
impl Service for Otterscan {
    async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        Otterscan::start(self, toxiproxy).await
    }

    async fn stop(&mut self) -> Result<()> {
        Otterscan::stop(self).await
    }

    fn is_running(&self) -> bool {
        Otterscan::is_running(self)
    }

    fn url(&self) -> Url {
        Otterscan::url(self)
    }

    fn external_url(&self) -> Url {
        Otterscan::external_url(self)
    }

    fn name(&self) -> String {
        "otterscan".to_string()
    }

    fn port(&self) -> u16 {
        OtterscanConst::EXTERNAL_PORT
    }

    /// No-op: Otterscan uses direct port binding, not ToxiProxy.
    async fn register(&mut self, _toxiproxy: &ToxiProxy, _: Option<u16>) -> Result<u16> {
        Ok(OtterscanConst::EXTERNAL_PORT)
    }
}
