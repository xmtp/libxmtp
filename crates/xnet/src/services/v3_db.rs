//! V3 Database (db) container management for node-go store.

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
    config::{DEFAULT_POSTGRES_PASSWORD, DEFAULT_V3_DB_IMAGE, V3_DB_CONTAINER_NAME, V3_DB_PORT},
    network::XNET_NETWORK_NAME,
    services::{
        ContainerState, Service, ToxiProxy, create_and_start_container, ensure_container_running,
        stop_and_remove_container,
    },
};

/// Manages a PostgreSQL Docker container for V3 node-go store.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
pub struct V3Db {
    /// The PostgreSQL image
    #[builder(default = DEFAULT_V3_DB_IMAGE.to_string())]
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

impl V3Db {
    /// Start the V3 database container.
    ///
    /// Registers itself with ToxiProxy for external access.
    /// If a container with the same name already exists, it will be reused.
    pub async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        let docker = Docker::connect_with_socket_defaults()?;

        let container_id = match ensure_container_running(&docker, V3_DB_CONTAINER_NAME).await? {
            ContainerState::Exists(id) => id,
            ContainerState::NotFound => {
                let options = CreateContainerOptionsBuilder::default().name(V3_DB_CONTAINER_NAME);

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

                create_and_start_container(&docker, V3_DB_CONTAINER_NAME, options, config).await?
            }
        };

        self.docker = Some(docker);
        self.container_id = Some(container_id);

        // Register with ToxiProxy
        let upstream = format!("{}:{}", V3_DB_CONTAINER_NAME, V3_DB_PORT);
        let port = toxiproxy.register("v3_db", upstream).await?;
        self.proxy_port = Some(port);

        Ok(())
    }

    /// Stop and remove the V3 database container.
    pub async fn stop(&mut self) -> Result<()> {
        if let (Some(docker), Some(id)) = (&self.docker, &self.container_id) {
            stop_and_remove_container(docker, id, V3_DB_CONTAINER_NAME).await?;
        }
        self.container_id = None;
        self.proxy_port = None;
        Ok(())
    }

    /// PostgreSQL connection URL for use within the docker network.
    pub fn url(&self) -> Url {
        Url::parse(&format!(
            "postgres://postgres:{}@{}:{}/postgres?sslmode=disable",
            self.password, V3_DB_CONTAINER_NAME, V3_DB_PORT
        ))
        .expect("valid URL")
    }

    /// PostgreSQL connection URL for external access (through ToxiProxy).
    pub fn external_url(&self) -> Option<Url> {
        self.proxy_port.map(|port| {
            Url::parse(&format!(
                "postgres://postgres:{}@localhost:{}/postgres?sslmode=disable",
                self.password, port
            ))
            .expect("valid URL")
        })
    }

    /// Get the ToxiProxy port for this service.
    pub fn proxy_port(&self) -> Option<u16> {
        self.proxy_port
    }

    /// Check if V3 database is running.
    pub fn is_running(&self) -> bool {
        self.container_id.is_some()
    }
}

#[async_trait]
impl Service for V3Db {
    async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        V3Db::start(self, toxiproxy).await
    }

    async fn stop(&mut self) -> Result<()> {
        V3Db::stop(self).await
    }

    fn is_running(&self) -> bool {
        V3Db::is_running(self)
    }

    fn url(&self) -> Url {
        V3Db::url(self)
    }

    fn external_url(&self) -> Url {
        self.external_url().unwrap_or_else(|| self.url())
    }

    fn name(&self) -> &'static str {
        "v3_db"
    }
}
