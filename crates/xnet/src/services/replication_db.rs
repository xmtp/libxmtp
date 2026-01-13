//! PostgreSQL (ReplicationDb) container management for D14n.

use async_trait::async_trait;
use bollard::{
    Docker,
    models::{ContainerCreateBody, HealthConfig, HostConfig},
    query_parameters::CreateContainerOptionsBuilder,
};
use bon::Builder;
use color_eyre::eyre::Result;

use crate::{
    config::{
        DEFAULT_POSTGRES_IMAGE, DEFAULT_POSTGRES_PASSWORD, POSTGRES_PORT,
        REPLICATION_DB_CONTAINER_NAME,
    },
    network::XNET_NETWORK_NAME,
    services::{
        ContainerState, Service, ToxiProxy, create_and_start_container, ensure_container_running,
        stop_and_remove_container,
    },
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

impl ReplicationDb {
    /// Start the PostgreSQL container.
    ///
    /// Registers itself with ToxiProxy for external access.
    /// If a container with the same name already exists, it will be reused.
    pub async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        let docker = Docker::connect_with_socket_defaults()?;

        let container_id = match ensure_container_running(&docker, REPLICATION_DB_CONTAINER_NAME)
            .await?
        {
            ContainerState::Exists(id) => id,
            ContainerState::NotFound => {
                let options = CreateContainerOptionsBuilder::default()
                    .name(REPLICATION_DB_CONTAINER_NAME)
                    .platform("linux/amd64");

                let config = ContainerCreateBody {
                    image: Some(self.image.clone()),
                    env: Some(vec![format!("POSTGRES_PASSWORD={}", self.password)]),
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

                create_and_start_container(&docker, REPLICATION_DB_CONTAINER_NAME, options, config)
                    .await?
            }
        };

        self.docker = Some(docker);
        self.container_id = Some(container_id);

        // Register with ToxiProxy
        let upstream = format!("{}:{}", REPLICATION_DB_CONTAINER_NAME, POSTGRES_PORT);
        let port = toxiproxy.register("replication_db", upstream).await?;
        self.proxy_port = Some(port);

        Ok(())
    }

    /// Stop and remove the PostgreSQL container.
    pub async fn stop(&mut self) -> Result<()> {
        if let (Some(docker), Some(id)) = (&self.docker, &self.container_id) {
            stop_and_remove_container(docker, id, REPLICATION_DB_CONTAINER_NAME).await?;
        }
        self.container_id = None;
        self.proxy_port = None;
        Ok(())
    }

    /// PostgreSQL connection URL for use within the docker network.
    pub fn url(&self) -> String {
        format!(
            "postgres://postgres:{}@{}:{}/postgres",
            self.password, REPLICATION_DB_CONTAINER_NAME, POSTGRES_PORT
        )
    }

    /// PostgreSQL connection URL for external access (through ToxiProxy).
    pub fn external_url(&self) -> Option<String> {
        self.proxy_port.map(|port| {
            format!(
                "postgres://postgres:{}@localhost:{}/postgres",
                self.password, port
            )
        })
    }

    /// Get the ToxiProxy port for this service.
    pub fn proxy_port(&self) -> Option<u16> {
        self.proxy_port
    }

    /// Check if PostgreSQL is running.
    pub fn is_running(&self) -> bool {
        self.container_id.is_some()
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

    fn url(&self) -> String {
        ReplicationDb::url(self)
    }

    fn external_url(&self) -> String {
        self.external_url().unwrap_or_else(|| self.url())
    }

    fn name(&self) -> &'static str {
        "replication_db"
    }
}
