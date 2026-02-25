//! Manage Static Resources on XNET (Chain, Gateway, ToxiProxy)
//! Each Service:
//! 1.) Starts a container (either pre-existing or pulling an image and starting)
//! 2.) registers itself with toxiproxy

mod anvil;
mod coredns;
mod gateway;
mod grafana;
mod history;
mod mlsdb;
mod node_go;
mod otterscan;
mod prometheus;
mod redis;
mod replication_db;
mod toxiproxy;
mod traefik;
mod traefik_config;
mod v3_db;
mod validation;
mod xmtpd;

use std::time::Duration;

pub use anvil::Anvil;
pub use coredns::CoreDns;
pub use gateway::Gateway;
pub use grafana::Grafana;
pub use history::HistoryServer;
use map_macro::hash_map;
pub use mlsdb::MlsDb;
pub use node_go::NodeGo;
pub use otterscan::Otterscan;
pub use prometheus::Prometheus;
pub use redis::Redis;
pub use replication_db::ReplicationDb;
pub use toxiproxy::{ProxyConfig, ToxiProxy, allocate_xmtpd_port};
pub use traefik::Traefik;
pub use traefik_config::TraefikConfig;
pub use v3_db::V3Db;
pub use validation::Validation;
pub use xmtpd::Xmtpd;

use crate::Config;
use crate::constants::ToxiProxy as ToxiProxyConst;
use async_trait::async_trait;
use bollard::{
    Docker,
    models::ContainerCreateBody,
    query_parameters::{
        CreateContainerOptionsBuilder, CreateImageOptionsBuilder, EventsOptions,
        RemoveContainerOptionsBuilder, StopContainerOptionsBuilder,
    },
    secret::PortBinding,
};
use color_eyre::eyre::{OptionExt, Result, eyre};
use futures::{StreamExt, TryStreamExt};
use tokio::time::timeout;
use tracing::info;
use url::Url;

/// Result of checking for an existing container.
pub enum ContainerState {
    /// Container exists (running or stopped) - contains the container ID
    Exists(String),
    /// Container does not exist - needs to be created
    NotFound,
}

fn db_connection_string(password: &str, host: &str) -> Url {
    Url::parse(&format!(
        "postgres://postgres:{}@{}/postgres?sslmode=disable",
        password, host
    ))
    .expect("valid postgres URL")
}

async fn wait_for_healthy_events<F, T>(start_fn: F, container_name: &str) -> Result<()>
where
    F: Future<Output = Result<T>>,
{
    let docker = Docker::connect_with_socket_defaults()?;
    let mut filters = std::collections::HashMap::new();
    filters.insert("container".to_string(), vec![container_name.to_string()]);
    filters.insert("event".to_string(), vec!["health_status".to_string()]);

    let mut stream = docker.events(Some(EventsOptions {
        filters: Some(filters),
        ..Default::default()
    }));

    start_fn.await?;
    let result = timeout(Duration::from_secs(10), async {
        while let Some(event) = stream.next().await {
            let event = event?;
            if let Some(status) = event.action
                && status == "health_status: healthy"
            {
                return Ok(());
            }
        }
        Err(eyre!("Event stream ended"))
    })
    .await;

    match result {
        Ok(inner) => inner,
        Err(_) => Err(eyre!(
            "Timeout waiting for {} to be healthy",
            container_name
        )),
    }
}

/// Check if a container exists and ensure it's running if it does.
///
/// Returns `ContainerState::Exists(id)` if the container was found (and started if needed),
/// or `ContainerState::NotFound` if the container needs to be created.
pub async fn ensure_container_running(
    docker: &Docker,
    container_name: &str,
) -> Result<ContainerState> {
    let inspect_result = docker.inspect_container(container_name, None).await;

    let container_info = match inspect_result {
        Ok(info) => info,
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => return Ok(ContainerState::NotFound),
        Err(e) => return Err(e.into()),
    };

    let container_id = container_info
        .id
        .ok_or_else(|| color_eyre::eyre::eyre!("Container has no ID"))?;

    let is_running = container_info
        .state
        .as_ref()
        .and_then(|s| s.running)
        .unwrap_or(false);

    let short_id = &container_id[..12.min(container_id.len())];

    if is_running {
        info!(
            "connected to existing container {}: {}",
            container_name, short_id
        );
    } else {
        info!(
            "starting stopped container {}: {}",
            container_name, short_id
        );
        docker.start_container(&container_id, None).await?;
    }

    Ok(ContainerState::Exists(container_id))
}

/// Pull a Docker image if it doesn't exist locally.
///
/// Parses the image string to extract the repository and tag,
/// then pulls the image from the registry.
pub async fn ensure_image_exists(docker: &Docker, image: &str) -> Result<()> {
    // Check if image exists locally
    if docker.inspect_image(image).await.is_ok() {
        info!("Image {} already exists locally", image);
        return Ok(());
    }

    info!("Pulling image: {}", image);

    // Parse image into repository and tag
    // Format: "registry/repo:tag" or "repo:tag" or "repo" (defaults to :latest)
    let (repo, tag) = if let Some((r, t)) = image.rsplit_once(':') {
        // Check if the colon is part of a port number (e.g., localhost:5000/repo)
        // by seeing if there's a slash after the colon
        if t.contains('/') {
            (image, "latest")
        } else {
            (r, t)
        }
    } else {
        (image, "latest")
    };

    let options = CreateImageOptionsBuilder::default()
        .from_image(repo)
        .tag(tag)
        .build();

    let _: Vec<_> = docker
        .create_image(Some(options), None, None)
        .try_collect()
        .await?;

    info!("Successfully pulled image: {}", image);
    Ok(())
}

pub fn expose(p: u16) -> Option<Vec<PortBinding>> {
    Some(vec![PortBinding {
        host_ip: Some("0.0.0.0".to_string()),
        host_port: Some(format!("{p}/tcp")),
    }])
}

pub fn expose_udp(p: u16) -> Option<Vec<PortBinding>> {
    Some(vec![PortBinding {
        host_ip: Some("0.0.0.0".to_string()),
        host_port: Some(format!("{p}/udp")),
    }])
}

pub fn expose_127(p: u16) -> Option<Vec<PortBinding>> {
    Some(vec![PortBinding {
        host_ip: Some("127.0.0.1".to_string()),
        host_port: Some(p.to_string()),
    }])
}

pub fn expose_127_udp(p: u16) -> Option<Vec<PortBinding>> {
    Some(vec![PortBinding {
        host_ip: Some("127.0.0.1".to_string()),
        host_port: Some(format!("{p}/udp")),
    }])
}

/// Create and start a new container.
///
/// Pulls the image if it doesn't exist locally, then creates and starts the container.
pub async fn create_and_start_container(
    docker: &Docker,
    container_name: &str,
    options: CreateContainerOptionsBuilder,
    mut config: ContainerCreateBody,
) -> Result<String> {
    // Pull image if needed
    if let Some(ref image) = config.image {
        ensure_image_exists(docker, image).await?;
    }
    let exposed: Option<Vec<String>> = config
        .host_config
        .as_ref()
        .and_then(|h| h.port_bindings.clone())
        .map(|p| p.keys().cloned().collect());
    config.exposed_ports = exposed;
    config.labels = Some(hash_map! {
        "com.xmtp.network".to_string() => "xnet".to_string()
    });
    let response = docker
        .create_container(Some(options.build()), config)
        .await?;
    docker.start_container(&response.id, None).await?;

    let short_id = &response.id[..12.min(response.id.len())];
    info!("Started container {}: {}", container_name, short_id);

    Ok(response.id)
}

/// Stop a container.
///
/// Helper to reduce boilerplate in service implementations.
/// Gracefully stops the container (with 10s timeout).
pub async fn stop_container(
    docker: &Docker,
    container_id: &str,
    container_name: &str,
) -> Result<()> {
    info!("stopping container {}: {}", container_name, container_id);

    let stop_opts = StopContainerOptionsBuilder::default().t(10).build();
    if let Err(e) = docker.stop_container(container_id, Some(stop_opts)).await {
        tracing::debug!("Could not stop container (may already be stopped): {}", e);
    }

    info!("Container {} stopped", container_name);
    Ok(())
}

/// Managed container state - provides common lifecycle management for Docker containers.
///
/// This struct encapsulates the shared state and lifecycle operations that all
/// service containers need, reducing duplication across service implementations.
#[derive(Debug, Default, Clone)]
pub struct ManagedContainer {
    /// Docker client connection
    docker: Option<Docker>,
    /// Container ID once started
    container_id: Option<String>,
    /// ToxiProxy port for external access
    proxy_port: Option<u16>,
}

impl ManagedContainer {
    /// Create a new uninitialized container manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Start or connect to an existing container.
    ///
    /// This method handles the complete container lifecycle:
    /// 1. Check if container exists (and start it if stopped)
    /// 2. Create and start new container if it doesn't exist
    /// 3. Store the Docker client and container ID
    ///
    /// Returns the container ID.
    pub async fn start_container(
        &mut self,
        container_name: &str,
        options: CreateContainerOptionsBuilder,
        config: ContainerCreateBody,
    ) -> Result<String> {
        let docker = Docker::connect_with_socket_defaults()?;

        let container_id = match ensure_container_running(&docker, container_name).await? {
            ContainerState::Exists(id) => id,
            ContainerState::NotFound => {
                create_and_start_container(&docker, container_name, options, config).await?
            }
        };

        self.docker = Some(docker);
        self.container_id = Some(container_id.clone());

        Ok(container_id)
    }

    /// Stop the managed container.
    pub async fn stop_container(&self, container_name: &str) -> Result<()> {
        if let (Some(docker), Some(id)) = (&self.docker, &self.container_id) {
            stop_container(docker, id, container_name).await?;
        }
        Ok(())
    }

    /// Stop and remove the managed container.
    ///
    /// After removal, the next call to `start_container` will create a fresh container.
    pub async fn remove_container(&mut self, container_name: &str) -> Result<()> {
        if let (Some(docker), Some(id)) = (&self.docker, &self.container_id) {
            stop_container(docker, id, container_name).await?;
            let remove_opts = RemoveContainerOptionsBuilder::default().force(true).build();
            docker.remove_container(id, Some(remove_opts)).await?;
        }
        self.container_id = None;
        Ok(())
    }

    /// Check if the container is currently running.
    pub fn is_running(&self) -> bool {
        self.container_id.is_some()
    }

    /// Set the ToxiProxy port for this container.
    pub fn set_proxy_port(&mut self, port: u16) {
        self.proxy_port = Some(port);
    }

    /// Get the ToxiProxy port for external access.
    pub fn proxy_port(&self) -> Option<u16> {
        self.proxy_port
    }

    /// Get the container ID if running.
    pub fn container_id(&self) -> Option<&str> {
        self.container_id.as_deref()
    }

    /// Get the Docker client if initialized.
    pub fn docker(&self) -> Option<&Docker> {
        self.docker.as_ref()
    }

    /// Get the container's IP address on a specific Docker network.
    pub async fn container_ip(&self, container_name: &str, network_name: &str) -> Result<String> {
        let docker = self
            .docker
            .as_ref()
            .ok_or_else(|| eyre!("Container not started"))?;

        let info = docker.inspect_container(container_name, None).await?;

        let networks = info
            .network_settings
            .ok_or_else(|| eyre!("No network settings"))?
            .networks
            .ok_or_else(|| eyre!("No networks"))?;

        let network = networks
            .get(network_name)
            .ok_or_else(|| eyre!("Not connected to {} network", network_name))?;

        network
            .ip_address
            .clone()
            .filter(|ip| !ip.is_empty())
            .ok_or_else(|| eyre!("No IP address assigned"))
    }
}

/// Common trait for all Docker container services.
///
/// This trait provides a unified interface for managing Docker containers,
/// allowing services to be used polymorphically via `Box<dyn Service>`.
#[async_trait]
pub trait Service: Send + Sync {
    /// Start the service container and register with ToxiProxy.
    ///
    /// The service should register itself with ToxiProxy to get an external port
    /// for client access.
    async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()>;

    /// Stop the service container.
    async fn stop(&mut self) -> Result<()>;

    /// Check if the service is currently running.
    fn is_running(&self) -> bool;

    /// Get the service URL for use within the docker network.
    fn url(&self) -> Url;

    /// Get the service URL for external access (through ToxiProxy).
    fn external_url(&self) -> Url;

    fn internal_proxy_host(&self) -> Result<String> {
        let toxi_port = self
            .external_url()
            .port()
            .ok_or_else(|| eyre!("service {} does not have toxi port assigned", self.name()))?;
        Ok(format!("{}:{}", ToxiProxyConst::CONTAINER_NAME, toxi_port))
    }

    async fn register(&mut self, toxiproxy: &ToxiProxy, port_override: Option<u16>) -> Result<u16> {
        let config = crate::Config::load_unchecked();
        let name = self.name();
        let url = <Self as Service>::url(self);
        let host = url.host().ok_or_eyre(format!("no host for {name}"))?;
        let mut upstream = format!("{host}");
        if let Some(p) = url.port() {
            upstream.push_str(&format!(":{p}"));
        }
        let port = if config.use_standard_ports {
            let port = port_override.unwrap_or(self.port());
            toxiproxy.register_at(self.name(), upstream, port).await?;
            port
        } else {
            toxiproxy.register(self.name(), upstream).await?
        };
        Ok(port)
    }

    /// Get the service name (for logging/identification).
    fn name(&self) -> String;

    fn port(&self) -> u16;

    /// Get the service hostname for DNS-based routing.
    ///
    /// Returns None for services that use direct port access (most services).
    /// Returns Some(hostname) for services using unified addressing (XMTPD, Gateway).
    ///
    /// Services with hostnames should use this in their url() and external_url() implementations
    /// to ensure consistent addressing from both host and Docker network contexts.
    fn hostname(&self) -> Option<String> {
        None
    }
}
