//! Manage the docker network where all the nodes and resources run
use bollard::{
    Docker,
    models::NetworkCreateRequest,
    query_parameters::{
        InspectNetworkOptions, ListContainersOptionsBuilder, ListVolumesOptions,
        RemoveContainerOptionsBuilder, RemoveVolumeOptions, StopContainerOptionsBuilder,
    },
};
use color_eyre::eyre::{Context, Error, Result};
use futures::future::try_join_all;
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// The name of the docker network used by xnet
pub const XNET_NETWORK_NAME: &str = "xnet";

/// Check if the Docker daemon is reachable.
pub async fn check_docker_available() -> Result<()> {
    let docker =
        Docker::connect_with_socket_defaults().context("Cannot connect to Docker socket")?;
    docker
        .ping()
        .await
        .context("Docker daemon is not responding")?;
    Ok(())
}

/// Utilities to manage the docker network
pub struct Network {
    docker: Docker,
}

impl Network {
    /// Create a new Network instance.
    /// Creates the xnet docker network if it doesn't already exist.
    pub async fn new() -> Result<Self> {
        info!("connecting to docker");
        let docker = Docker::connect_with_socket_defaults()?;
        info!("connected");

        match docker
            .inspect_network(XNET_NETWORK_NAME, Some(InspectNetworkOptions::default()))
            .await
        {
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => {
                // Network doesn't exist, create it
                info!("Creating network '{}'", XNET_NETWORK_NAME);
                let config = NetworkCreateRequest {
                    name: XNET_NETWORK_NAME.to_string(),
                    driver: Some("bridge".to_string()),
                    enable_ipv4: Some(true),
                    ..Default::default()
                };
                docker.create_network(config).await?;
            }
            Err(e) => return Err(e.into()),
            _ => (),
        }
        tracing::info!("Ok");
        Ok(Self { docker })
    }

    /// Delete all resources on xnet (containers, volumes, and the network itself)
    pub async fn delete_all(&self) -> Result<()> {
        let mut filters = HashMap::new();
        filters.insert("network".to_string(), vec![XNET_NETWORK_NAME]);

        let options = ListContainersOptionsBuilder::default()
            .all(true)
            .filters(&filters)
            .build();

        let containers = self.docker.list_containers(Some(options)).await?;

        let mut futures = Vec::new();
        for container in &containers {
            let fut = async {
                let id = container.id.as_deref().unwrap_or("unknown");
                let names = container
                    .names
                    .as_ref()
                    .map(|n| n.join(", "))
                    .unwrap_or_else(|| "unnamed".to_string());

                info!(
                    "Stopping and removing container: {} ({}..{})",
                    names,
                    &id[..3],
                    &id[id.len() - 3..]
                );

                let stop_options = StopContainerOptionsBuilder::default().t(10).build();
                if let Err(e) = self.docker.stop_container(id, Some(stop_options)).await {
                    debug!(
                        "Could not stop container {} (may already be stopped): {}",
                        id, e
                    );
                }

                // Remove the container with force and volume cleanup
                let remove_options = RemoveContainerOptionsBuilder::default()
                    .force(true)
                    .v(true) // Remove associated anonymous volumes
                    .build();

                self.docker
                    .remove_container(id, Some(remove_options))
                    .await
                    .wrap_err("unable to remove container")?;
                Ok::<_, Error>(())
            };
            futures.push(fut);
        }
        try_join_all(futures).await?;

        // List and remove volumes with xnet label
        let volumes = self
            .docker
            .list_volumes(Some(ListVolumesOptions::default()))
            .await?;

        if let Some(volumes) = volumes.volumes {
            for volume in volumes {
                // Check if volume has xnet-related labels or name
                let should_remove =
                    volume.name.starts_with("xnet") || volume.labels.contains_key("xnet");

                if should_remove {
                    info!("Removing volume: {}", volume.name);
                    if let Err(e) = self
                        .docker
                        .remove_volume(&volume.name, Some(RemoveVolumeOptions { force: true }))
                        .await
                    {
                        warn!("Could not remove volume {}: {}", volume.name, e);
                    }
                }
            }
        }

        // Remove the network itself
        info!("Removing network '{}'", XNET_NETWORK_NAME);
        self.docker
            .remove_network(XNET_NETWORK_NAME)
            .await
            .wrap_err("unable to remove`xnet` network")?;

        // Clean up Traefik dynamic config file
        let traefik_config_path = std::path::Path::new("/tmp/xnet/traefik/dynamic.yml");
        if traefik_config_path.exists() {
            info!(
                "Removing Traefik dynamic config: {}",
                traefik_config_path.display()
            );
            if let Err(e) = std::fs::remove_file(traefik_config_path) {
                warn!("Could not remove Traefik config: {}", e);
            }
        }

        info!("All xnet resources deleted");
        Ok(())
    }

    /// Stop all containers on the xnet network without removing them.
    pub async fn down(&self) -> Result<()> {
        let mut filters = HashMap::new();
        filters.insert("network".to_string(), vec![XNET_NETWORK_NAME.to_string()]);

        let options = ListContainersOptionsBuilder::default()
            .filters(&filters)
            .build();

        let containers = self.docker.list_containers(Some(options)).await?;

        for container in &containers {
            let id = container.id.as_deref().unwrap_or("unknown");
            let names = container
                .names
                .as_ref()
                .map(|n| n.join(", "))
                .unwrap_or_else(|| "unnamed".to_string());

            info!(
                "Stopping container: {} ({}..{})",
                names,
                &id[..3],
                &id[id.len() - 3..]
            );

            let stop_options = StopContainerOptionsBuilder::default().t(10).build();
            if let Err(e) = self.docker.stop_container(id, Some(stop_options)).await {
                debug!(
                    "Could not stop container {} (may already be stopped): {}",
                    id, e
                );
            }
        }

        info!("All xnet containers stopped");
        Ok(())
    }

    /// List the resources on XNet
    pub async fn list(&self) -> Result<()> {
        // List containers on the network
        let mut filters = HashMap::new();
        filters.insert("network".to_string(), vec![XNET_NETWORK_NAME.to_string()]);

        let options = ListContainersOptionsBuilder::default()
            .all(true)
            .filters(&filters)
            .build();

        let containers = self.docker.list_containers(Some(options)).await?;

        info!("Containers on '{}' network:", XNET_NETWORK_NAME);
        for container in &containers {
            let id = container.id.as_deref().unwrap_or("unknown");
            let names = container
                .names
                .as_ref()
                .map(|n| n.join(", "))
                .unwrap_or_else(|| "unnamed".to_string());
            let state = container
                .state
                .as_ref()
                .map(|s| format!("{:?}", s))
                .unwrap_or_else(|| "unknown".to_string());
            info!("  {} ({}) - {}", names, id, state);
        }

        if containers.is_empty() {
            info!("  (no containers)");
        }

        Ok(())
    }
}
