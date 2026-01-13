//! PostgreSQL (ReplicationDb) container management for D14n.

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
    constants::{
        DEFAULT_POSTGRES_IMAGE, DEFAULT_POSTGRES_PASSWORD, POSTGRES_PORT,
        REPLICATION_DB_CONTAINER_NAME,
    },
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy},
};

/// Manages a PostgreSQL Docker container for D14n replication.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
pub struct ReplicationDb {
    /// The PostgreSQL image
    #[builder(default = DEFAULT_POSTGRES_IMAGE.to_string())]
    image: String,

    /// PostgreSQL password
    #[builder(default = DEFAULT_POSTGRES_PASSWORD.to_string())]
    password: String,

    /// Managed container state
    #[builder(default = ManagedContainer::new())]
    container: ManagedContainer,
}

impl ReplicationDb {
    /// Start the PostgreSQL container.
    ///
    /// Registers itself with ToxiProxy for external access.
    /// If a container with the same name already exists, it will be reused.
    pub async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        let options =
            CreateContainerOptionsBuilder::default().name(REPLICATION_DB_CONTAINER_NAME);

        let config = ContainerCreateBody {
            image: Some(self.image.clone()),
            env: Some(vec![
                format!("POSTGRES_PASSWORD={}", self.password),
                format!("PGPORT={POSTGRES_PORT}"),
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
            .start_container(REPLICATION_DB_CONTAINER_NAME, options, config)
            .await?;

        let port = self.register(toxiproxy, None).await?;
        self.container.set_proxy_port(port);

        Ok(())
    }

    /// Stop the PostgreSQL container.
    pub async fn stop(&mut self) -> Result<()> {
        self.container
            .stop_container(REPLICATION_DB_CONTAINER_NAME)
            .await
    }

    /// PostgreSQL connection URL for use within the docker network.
    pub fn url(&self) -> Url {
        Url::parse(&format!(
            "postgres://postgres:{}@{}:{}/postgres",
            self.password, REPLICATION_DB_CONTAINER_NAME, POSTGRES_PORT
        ))
        .expect("valid URL")
    }

    /// PostgreSQL connection URL for external access (through ToxiProxy).
    pub fn external_url(&self) -> Option<Url> {
        self.container.proxy_port().map(|port| {
            Url::parse(&format!(
                "postgres://postgres:{}@localhost:{}/postgres",
                self.password, port
            ))
            .expect("valid URL")
        })
    }

    /// Get the ToxiProxy port for this service.
    pub fn proxy_port(&self) -> Option<u16> {
        self.container.proxy_port()
    }

    /// Check if PostgreSQL is running.
    pub fn is_running(&self) -> bool {
        self.container.is_running()
    }
}

#[async_trait]
impl Service for ReplicationDb {
    async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        ReplicationDb::start(self, toxiproxy).await
    }

    async fn stop(&mut self) -> Result<()> {
        ReplicationDb::stop(self).await
    }

    fn is_running(&self) -> bool {
        ReplicationDb::is_running(self)
    }

    fn url(&self) -> Url {
        ReplicationDb::url(self)
    }

    fn external_url(&self) -> Url {
        self.external_url().unwrap_or_else(|| self.url())
    }

    fn name(&self) -> String {
        "replication_db".to_string()
    }

    fn port(&self) -> u16 {
        POSTGRES_PORT
    }
}
