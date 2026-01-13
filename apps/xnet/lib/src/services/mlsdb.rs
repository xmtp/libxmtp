//! MLS Database container management for MLS store.

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
    constants::{MlsDb as MlsDbConst, POSTGRES_PASSWORD, ReplicationDb as ReplicationDbConst},
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy},
};

/// Manages a PostgreSQL Docker container for MLS store.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
pub struct MlsDb {
    /// The PostgreSQL image
    #[builder(default = MlsDbConst::IMAGE.to_string())]
    image: String,

    /// PostgreSQL password
    #[builder(default = POSTGRES_PASSWORD.to_string())]
    password: String,

    /// Managed container state
    #[builder(default = ManagedContainer::new())]
    container: ManagedContainer,
}

impl MlsDb {
    /// Start the MLS database container.
    ///
    /// Registers itself with ToxiProxy for external access.
    /// If a container with the same name already exists, it will be reused.
    pub async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        let options = CreateContainerOptionsBuilder::default().name(MlsDbConst::CONTAINER_NAME);

        let config = ContainerCreateBody {
            image: Some(self.image.clone()),
            env: Some(vec![
                format!("POSTGRES_PASSWORD={}", self.password),
                format!("PGPORT={}", ReplicationDbConst::PORT),
            ]),
            healthcheck: Some(HealthConfig {
                test: Some(vec![
                    "CMD-SHELL".to_string(),
                    "pg_isready -U postgres".to_string(),
                ]),
                interval: Some(5_000_000_000),
                timeout: Some(5_000_000_000),
                retries: Some(5),
                start_period: None,
                start_interval: None,
            }),
            host_config: Some(HostConfig {
                network_mode: Some(XNET_NETWORK_NAME.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        self.container
            .start_container(MlsDbConst::CONTAINER_NAME, options, config)
            .await?;

        let port = self.register(toxiproxy, None).await?;
        self.container.set_proxy_port(port);

        Ok(())
    }

    /// Stop the MLS database container.
    pub async fn stop(&mut self) -> Result<()> {
        self.container
            .stop_container(MlsDbConst::CONTAINER_NAME)
            .await
    }

    /// PostgreSQL connection URL for use within the docker network.
    pub fn url(&self) -> Url {
        Url::parse(&format!(
            "postgres://postgres:{}@{}:{}/postgres?sslmode=disable",
            self.password,
            MlsDbConst::CONTAINER_NAME,
            ReplicationDbConst::PORT
        ))
        .expect("valid URL")
    }

    /// PostgreSQL connection URL for external access (through ToxiProxy).
    pub fn external_url(&self) -> Option<Url> {
        self.container.proxy_port().map(|port| {
            Url::parse(&format!(
                "postgres://postgres:{}@localhost:{}/postgres?sslmode=disable",
                self.password, port
            ))
            .expect("valid URL")
        })
    }

    /// Get the ToxiProxy port for this service.
    pub fn proxy_port(&self) -> Option<u16> {
        self.container.proxy_port()
    }

    /// Check if MLS database is running.
    pub fn is_running(&self) -> bool {
        self.container.is_running()
    }
}

#[async_trait]
impl Service for MlsDb {
    async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        MlsDb::start(self, toxiproxy).await
    }

    async fn stop(&mut self) -> Result<()> {
        MlsDb::stop(self).await
    }

    fn is_running(&self) -> bool {
        MlsDb::is_running(self)
    }

    fn url(&self) -> Url {
        MlsDb::url(self)
    }

    fn external_url(&self) -> Url {
        self.external_url().unwrap_or_else(|| self.url())
    }

    fn name(&self) -> String {
        "mls_db".to_string()
    }

    fn port(&self) -> u16 {
        MlsDbConst::PORT
    }
}
