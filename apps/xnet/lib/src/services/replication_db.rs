//! PostgreSQL (ReplicationDb) container management for D14n.

use crate::types::XmtpdNode;
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
    constants::ReplicationDb as ReplicationDbConst,
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy},
};

/// Manages a PostgreSQL Docker container for D14n replication.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
pub struct ReplicationDb {
    /// The PostgreSQL image
    #[builder(default = ReplicationDbConst::IMAGE.to_string())]
    image: String,

    /// Managed container state
    #[builder(default = ManagedContainer::new())]
    container: ManagedContainer,

    /// the node this db is attached to
    node: XmtpdNode,
}

impl ReplicationDb {
    /// Start the PostgreSQL container.
    ///
    /// Registers itself with ToxiProxy for external access.
    /// If a container with the same name already exists, it will be reused.
    pub async fn start(&mut self) -> Result<()> {
        let name = self.name();
        let options = CreateContainerOptionsBuilder::default().name(&name);

        let config = ContainerCreateBody {
            image: Some(self.image.clone()),
            env: Some(vec![
                format!("POSTGRES_PASSWORD={}", ReplicationDbConst::PASSWORD),
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
            .start_container(&name, options, config)
            .await?;
        Ok(())
    }

    /// Stop the PostgreSQL container.
    pub async fn stop(&mut self) -> Result<()> {
        self.container.stop_container(&self.name()).await
    }

    /// PostgreSQL connection URL for use within the docker network.
    pub fn url(&self) -> Url {
        Url::parse(&format!(
            "postgres://postgres:xmtp@{}:{}/postgres?sslmode=disable",
            self.name(),
            ReplicationDbConst::PORT,
        ))
        .expect("valid URL")
    }

    /// PostgreSQL connection URL for external access (through ToxiProxy).
    pub fn external_url(&self) -> Option<Url> {
        self.container.proxy_port().map(|port| {
            Url::parse(&format!(
                "postgres://postgres:xmtp@localhost:{}/postgres",
                self.port()
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
    async fn start(&mut self, _toxiproxy: &ToxiProxy) -> Result<()> {
        ReplicationDb::start(self).await
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
        format!("xmtpd-db-{}", self.node.id())
    }

    fn port(&self) -> u16 {
        ReplicationDbConst::PORT
    }
}
