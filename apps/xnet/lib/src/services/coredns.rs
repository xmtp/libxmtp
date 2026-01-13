//! CoreDNS container management for local DNS resolution.
//!
//! Provides DNS resolution for *.xmtpd.local → 127.0.0.1
//! Runs on port 5353 (non-privileged) to avoid conflicts with system DNS.

use async_trait::async_trait;
use bollard::{
    Docker,
    models::{ContainerCreateBody, HostConfig, Mount, MountTypeEnum},
    query_parameters::CreateContainerOptionsBuilder,
};
use bon::Builder;
use color_eyre::eyre::Result;
use map_macro::hash_map;
use std::{collections::hash_map, fs};
use url::Url;

use crate::{
    constants::{COREDNS_CONTAINER_NAME, COREDNS_PORT},
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy, expose, expose_127, expose_udp},
};

const DEFAULT_COREDNS_IMAGE: &str = "coredns/coredns";
const DEFAULT_COREDNS_VERSION: &str = "1.11.1";

/// CoreDNS Corefile configuration
/// Listens on both TCP and UDP for compatibility with macOS DNS resolver
const COREFILE: &str = r#".:5354 {
    log
    errors

    # Resolve *.xmtpd.local to 127.0.0.1
    template IN A xmtpd.local {
        answer "{{ .Name }} 60 IN A 127.0.0.1"
    }

    # Forward everything else to host DNS
    forward . /etc/resolv.conf

    cache 30
}
"#;

/// Manages a CoreDNS Docker container for local DNS resolution.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
pub struct CoreDns {
    /// The CoreDNS image
    #[builder(default = DEFAULT_COREDNS_IMAGE.to_string())]
    image: String,

    /// The version tag
    #[builder(default = DEFAULT_COREDNS_VERSION.to_string())]
    version: String,

    /// Path to the Corefile configuration
    #[builder(default = "/tmp/xnet/coredns/Corefile".to_string())]
    corefile_path: String,

    /// Managed container state
    #[builder(default = ManagedContainer::new())]
    container: ManagedContainer,
}

impl CoreDns {
    /// Start the CoreDNS container.
    ///
    /// Creates the Corefile and mounts it into the container.
    /// CoreDNS does not use ToxiProxy as DNS doesn't need fault injection.
    pub async fn start(&mut self, _toxiproxy: &ToxiProxy) -> Result<()> {
        // Create Corefile
        if let Some(parent) = std::path::Path::new(&self.corefile_path).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.corefile_path, COREFILE)?;
        info!("Created Corefile at {}", self.corefile_path);

        let options = CreateContainerOptionsBuilder::default().name(COREDNS_CONTAINER_NAME);

        // Map host port 5353 to container port 5353 (both TCP and UDP)
        // TCP works better than UDP for Docker Desktop on macOS
        let port_bindings = hash_map! {
            // format!("53/udp").to_string() => expose_127(COREDNS_PORT),
            format!("5354/udp").to_string() => expose_udp(COREDNS_PORT),
            format!("5354/tcp").to_string() => expose(COREDNS_PORT),
        };

        let config = ContainerCreateBody {
            image: Some(format!("{}:{}", self.image, self.version)),
            cmd: Some(vec![
                "-conf".to_string(),
                "/etc/coredns/Corefile".to_string(),
            ]),
            exposed_ports: Some(vec!["5353/udp".to_string(), "5353/tcp".into()]),
            host_config: Some(HostConfig {
                network_mode: Some(XNET_NETWORK_NAME.to_string()),
                port_bindings: Some(port_bindings),
                mounts: Some(vec![Mount {
                    target: Some("/etc/coredns/Corefile".to_string()),
                    source: Some(self.corefile_path.clone()),
                    typ: Some(MountTypeEnum::BIND),
                    read_only: Some(true),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            ..Default::default()
        };

        self.container
            .start_container(COREDNS_CONTAINER_NAME, options, config)
            .await?;

        info!("CoreDNS started on port {}", COREDNS_PORT);
        Ok(())
    }

    /// Stop the CoreDNS container.
    pub async fn stop(&mut self) -> Result<()> {
        self.container.stop_container(COREDNS_CONTAINER_NAME).await
    }

    /// Check if CoreDNS is running.
    pub fn is_running(&self) -> bool {
        self.container.is_running()
    }

    /// Get the CoreDNS server address (for host queries).
    pub fn dns_server(&self) -> String {
        format!("127.0.0.1:{}", COREDNS_PORT)
    }
}

#[async_trait]
impl Service for CoreDns {
    async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        CoreDns::start(self, toxiproxy).await
    }

    async fn stop(&mut self) -> Result<()> {
        CoreDns::stop(self).await
    }

    fn is_running(&self) -> bool {
        CoreDns::is_running(self)
    }

    fn url(&self) -> Url {
        Url::parse(&format!("dns://127.0.0.1:{}", COREDNS_PORT)).expect("valid URL")
    }

    fn external_url(&self) -> Url {
        self.url()
    }

    fn name(&self) -> String {
        "coredns".to_string()
    }

    fn port(&self) -> u16 {
        COREDNS_PORT
    }

    // CoreDNS doesn't use ToxiProxy
    async fn register(&mut self, _toxiproxy: &ToxiProxy, _: Option<u16>) -> Result<u16> {
        Ok(COREDNS_PORT)
    }
}
