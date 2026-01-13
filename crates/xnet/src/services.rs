//! Manage Static Resources on XNET (Chain, Gateway, ToxiProxy)

mod anvil;
mod gateway;
mod history;
mod mlsdb;
mod node_go;
mod redis;
mod replication_db;
mod toxiproxy;
mod v3_db;
mod validation;

pub use anvil::Anvil;
pub use gateway::Gateway;
pub use history::HistoryServer;
pub use mlsdb::MlsDb;
pub use node_go::NodeGo;
pub use redis::Redis;
pub use replication_db::ReplicationDb;
pub use toxiproxy::{ProxyConfig, ToxiProxy, allocate_port, reset_port_allocator};
pub use v3_db::V3Db;
pub use validation::Validation;

use async_trait::async_trait;
use bollard::{
    Docker,
    models::ContainerCreateBody,
    query_parameters::{
        CreateContainerOptionsBuilder, CreateImageOptionsBuilder, RemoveContainerOptionsBuilder,
        StopContainerOptionsBuilder,
    },
};
use color_eyre::eyre::Result;
use futures::{StreamExt, TryStreamExt};
use tracing::info;

/// Result of checking for an existing container.
pub enum ContainerState {
    /// Container exists (running or stopped) - contains the container ID
    Exists(String),
    /// Container does not exist - needs to be created
    NotFound,
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
            "Reusing existing container {}: {}",
            container_name, short_id
        );
    } else {
        info!(
            "Starting existing container {}: {}",
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

/// Create and start a new container.
///
/// Pulls the image if it doesn't exist locally, then creates and starts the container.
pub async fn create_and_start_container(
    docker: &Docker,
    container_name: &str,
    options: CreateContainerOptionsBuilder,
    config: ContainerCreateBody,
) -> Result<String> {
    // Pull image if needed
    if let Some(ref image) = config.image {
        ensure_image_exists(docker, image).await?;
    }

    info!("Creating container: {}", container_name);

    let response = docker
        .create_container(Some(options.build()), config)
        .await?;
    docker.start_container(&response.id, None).await?;

    let short_id = &response.id[..12.min(response.id.len())];
    info!("Started container {}: {}", container_name, short_id);

    Ok(response.id)
}

/// Stop and remove a container.
///
/// Helper to reduce boilerplate in service implementations.
/// Gracefully stops the container (with 10s timeout) before removing it.
pub async fn stop_and_remove_container(
    docker: &Docker,
    container_id: &str,
    container_name: &str,
) -> Result<()> {
    info!("Stopping container {}: {}", container_name, container_id);

    let stop_opts = StopContainerOptionsBuilder::default().t(10).build();
    if let Err(e) = docker.stop_container(container_id, Some(stop_opts)).await {
        tracing::debug!("Could not stop container (may already be stopped): {}", e);
    }

    let remove_opts = RemoveContainerOptionsBuilder::default()
        .force(true)
        .v(true)
        .build();
    docker
        .remove_container(container_id, Some(remove_opts))
        .await?;

    info!("Container {} removed", container_name);
    Ok(())
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

    /// Stop and remove the service container.
    async fn stop(&mut self) -> Result<()>;

    /// Check if the service is currently running.
    fn is_running(&self) -> bool;

    /// Get the service URL for use within the docker network.
    fn url(&self) -> String;

    /// Get the service URL for external access (through ToxiProxy).
    fn external_url(&self) -> String;

    /// Get the service name (for logging/identification).
    fn name(&self) -> &'static str;
}
