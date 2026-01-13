//! Traefik reverse proxy container management.
//!
//! Provides hostname-based routing for unified addressing (same hostnames work
//! from host and within Docker network).

use async_trait::async_trait;
use bollard::{
    Docker,
    container::NetworkingConfig,
    models::{
        ContainerCreateBody, EndpointSettings, HostConfig, Mount, MountTypeEnum,
        NetworkConnectRequest,
    },
    query_parameters::CreateContainerOptionsBuilder,
};
use bon::Builder;
use color_eyre::eyre::Result;
use map_macro::hash_map;
use std::{collections::HashMap, fs};
use url::Url;

use crate::{
    Config,
    config::NodeToml,
    constants::{MAX_XMTPD_NODES, Traefik as TraefikConst, Xmtpd as XmtpdConst},
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy, TraefikConfig, expose, expose_127},
};

/// Traefik static configuration (traefik.yml)
/// Listens on both port 80 and 443 since gRPC clients may default to 443
const TRAEFIK_STATIC_CONFIG: &str = r#"# Traefik static configuration
entryPoints:
  http:
    address: ":80"
    http2:
      maxConcurrentStreams: 250
    transport:
      respondingTimeouts:
        readTimeout: 0s
  https:
    address: ":443"
    http2:
      maxConcurrentStreams: 250
    transport:
      respondingTimeouts:
        readTimeout: 0s
  traefik:
    address: ":8080"

api:
  dashboard: true
  insecure: true

providers:
  file:
    filename: /etc/traefik/dynamic.yml
    watch: true

log:
  level: INFO

accessLog: {}
"#;

/// Manages a Traefik Docker container for reverse proxy routing.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
pub struct Traefik {
    /// The Traefik image
    #[builder(default = TraefikConst::IMAGE.to_string())]
    image: String,

    /// The version tag
    #[builder(default = TraefikConst::VERSION.to_string())]
    version: String,

    /// Path to static config file
    #[builder(default = "/tmp/xnet/traefik/traefik.yml".to_string())]
    static_config_path: String,

    /// Path to dynamic config file (managed by TraefikConfig)
    #[builder(default = "/tmp/xnet/traefik/dynamic.yml".to_string())]
    dynamic_config_path: String,

    /// Managed container state
    #[builder(default = ManagedContainer::new())]
    container: ManagedContainer,
}

impl Traefik {
    /// Start the Traefik container with pre-allocated network aliases.
    ///
    /// Pre-allocates node0.xmtpd.local through node{MAX-1}.xmtpd.local plus
    /// gateway.xmtpd.local. This allows Docker's internal DNS to resolve these
    /// hostnames to the Traefik container for container-to-container routing.
    ///
    /// Traefik does not use ToxiProxy as it's the entry point for all HTTP traffic.
    pub async fn start(&mut self, _toxiproxy: &ToxiProxy) -> Result<()> {
        // Create config directory
        let config_dir = std::path::Path::new(&self.static_config_path)
            .parent()
            .unwrap();
        fs::create_dir_all(config_dir)?;

        // Write static config
        fs::write(&self.static_config_path, TRAEFIK_STATIC_CONFIG)?;
        info!(
            "Created Traefik static config at {}",
            self.static_config_path
        );

        // Ensure dynamic config exists (will be managed by TraefikConfig)
        if !std::path::Path::new(&self.dynamic_config_path).exists() {
            fs::write(&self.dynamic_config_path, "# Dynamic routes\n")?;
        }

        let options = CreateContainerOptionsBuilder::default().name(TraefikConst::CONTAINER_NAME);

        // Map host ports - expose 80 and 443 for HTTP/gRPC traffic
        let port_bindings = hash_map! {
            "80/tcp".to_string() => expose(TraefikConst::HTTP_PORT),
            "443/tcp".to_string() => expose(443),
            "8080/tcp".to_string() => expose_127(TraefikConst::DASHBOARD_PORT),
        };

        // Pre-allocate all possible XMTPD node hostnames + gateway
        // Node IDs increment by XmtpdConst::NODE_ID_INCREMENT (100), so: node100, node200, node300, ...
        let mut aliases = vec![TraefikConst::CONTAINER_NAME.to_string()];
        for i in 1..=MAX_XMTPD_NODES {
            let node_id = i * XmtpdConst::NODE_ID_INCREMENT as usize;
            aliases.push(format!("node{}.xmtpd.local", node_id));
        }
        let xnet_config = Config::load()?;
        for NodeToml { name, .. } in xnet_config.xmtpd_nodes {
            if let Some(n) = name {
                aliases.push(format!("{}.xmtpd.local", n));
            }
        }

        // Build networking config with aliases
        let mut endpoints_config = HashMap::new();
        endpoints_config.insert(
            XNET_NETWORK_NAME.to_string(),
            EndpointSettings {
                aliases: Some(aliases.clone()),
                ..Default::default()
            },
        );

        let config = ContainerCreateBody {
            image: Some(format!("{}:{}", self.image, self.version)),
            host_config: Some(HostConfig {
                network_mode: Some(XNET_NETWORK_NAME.to_string()),
                port_bindings: Some(port_bindings),
                mounts: Some(vec![
                    Mount {
                        target: Some("/etc/traefik/traefik.yml".to_string()),
                        source: Some(self.static_config_path.clone()),
                        typ: Some(MountTypeEnum::BIND),
                        read_only: Some(true),
                        ..Default::default()
                    },
                    Mount {
                        target: Some("/etc/traefik/dynamic.yml".to_string()),
                        source: Some(self.dynamic_config_path.clone()),
                        typ: Some(MountTypeEnum::BIND),
                        read_only: Some(false),
                        ..Default::default()
                    },
                ]),
                ..Default::default()
            }),
            networking_config: Some(NetworkingConfig { endpoints_config }.into()),
            ..Default::default()
        };

        self.container
            .start_container(TraefikConst::CONTAINER_NAME, options, config)
            .await?;

        info!(
            "Traefik started with {} pre-allocated hostname aliases",
            aliases.len()
        );
        info!(
            "Traefik dashboard: http://localhost:{}",
            TraefikConst::DASHBOARD_PORT
        );
        Ok(())
    }

    /// Stop the Traefik container.
    pub async fn stop(&mut self) -> Result<()> {
        self.container
            .stop_container(TraefikConst::CONTAINER_NAME)
            .await
    }

    /// Check if Traefik is running.
    pub fn is_running(&self) -> bool {
        self.container.is_running()
    }

    /// Get the Traefik HTTP endpoint.
    pub fn http_url(&self) -> Url {
        Url::parse(&format!("http://localhost:{}", TraefikConst::HTTP_PORT)).expect("valid URL")
    }

    /// Get the Traefik dashboard URL.
    pub fn dashboard_url(&self) -> Url {
        Url::parse(&format!(
            "http://localhost:{}",
            TraefikConst::DASHBOARD_PORT
        ))
        .expect("valid URL")
    }

    /// Get the path to the dynamic config file.
    pub fn dynamic_config_path(&self) -> &str {
        &self.dynamic_config_path
    }

    /// Get the Traefik container's IP address on the Docker network.
    pub async fn container_ip(&self) -> Result<String> {
        self.container
            .container_ip(TraefikConst::CONTAINER_NAME, XNET_NETWORK_NAME)
            .await
    }
}

#[async_trait]
impl Service for Traefik {
    async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        Traefik::start(self, toxiproxy).await
    }

    async fn stop(&mut self) -> Result<()> {
        Traefik::stop(self).await
    }

    fn is_running(&self) -> bool {
        Traefik::is_running(self)
    }

    fn url(&self) -> Url {
        self.http_url()
    }

    fn external_url(&self) -> Url {
        self.http_url()
    }

    fn name(&self) -> String {
        "traefik".to_string()
    }

    fn port(&self) -> u16 {
        TraefikConst::HTTP_PORT
    }

    // Traefik doesn't use ToxiProxy (it's the entry point)
    async fn register(&mut self, _toxiproxy: &ToxiProxy, _: Option<u16>) -> Result<u16> {
        Ok(TraefikConst::HTTP_PORT)
    }
}
