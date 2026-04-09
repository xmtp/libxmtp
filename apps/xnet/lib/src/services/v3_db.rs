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
    constants::{POSTGRES_PASSWORD, ReplicationDb as ReplicationDbConst, V3Db as V3DbConst},
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy},
};

/// Manages a PostgreSQL Docker container for V3 node-go store.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
pub struct V3Db {
    /// The PostgreSQL image
    #[builder(default = V3DbConst::IMAGE.to_string())]
    image: String,

    /// PostgreSQL password
    #[builder(default = POSTGRES_PASSWORD.to_string())]
    password: String,

    /// Managed container state
    #[builder(default = ManagedContainer::new())]
    container: ManagedContainer,
}

impl V3Db {
    /// Start the V3 database container.
    ///
    /// Registers itself with ToxiProxy for external access.
    /// If a container with the same name already exists, it will be reused.
    pub async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        let options = CreateContainerOptionsBuilder::default().name(V3DbConst::CONTAINER_NAME);

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
            .start_container(V3DbConst::CONTAINER_NAME, options, config)
            .await?;

        let port = self.register(toxiproxy, None).await?;
        self.container.set_proxy_port(port);

        Ok(())
    }

    /// Stop the V3 database container.
    pub async fn stop(&mut self) -> Result<()> {
        self.container
            .stop_container(V3DbConst::CONTAINER_NAME)
            .await
    }

    /// PostgreSQL connection URL for use within the docker network.
    pub fn url(&self) -> Url {
        Url::parse(&format!(
            "postgres://postgres:{}@{}:{}/postgres?sslmode=disable",
            self.password,
            V3DbConst::CONTAINER_NAME,
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

    /// Check if V3 database is running.
    pub fn is_running(&self) -> bool {
        self.container.is_running()
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

    fn name(&self) -> String {
        "v3_db".to_string()
    }

    fn port(&self) -> u16 {
        V3DbConst::PORT
    }
}
