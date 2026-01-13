//! CoreDNS container management for local DNS resolution.
//!
//! Provides two DNS endpoints:
//! - Port 5354 (host): Resolves *.xmtpd.local → 127.0.0.1 for host machine access
//! - Port 53 (container): Resolves *.xmtpd.local → xnet-traefik for container-to-container access

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
    constants::CoreDns as CoreDnsConst,
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy, expose, expose_udp},
};

/// Generate CoreDNS Corefile configuration.
/// Two server blocks:
/// - Port 5354: For host machine, resolves *.xmtpd.local to 127.0.0.1
/// - Port 53: For containers, resolves *.xmtpd.local to Traefik's IP
fn generate_corefile(traefik_ip: &str) -> String {
    format!(
        r#"# Host-facing DNS (port 5354)
# Resolves *.xmtpd.local to 127.0.0.1 for host machine access via Traefik on localhost
.:5354 {{
    log
    errors

    template IN A xmtpd.local {{
        answer "{{{{ .Name }}}} 60 IN A 127.0.0.1"
    }}

    forward . /etc/resolv.conf
    cache 30
}}

# Container-facing DNS (port 53)
# Resolves *.xmtpd.local to Traefik's container IP for container-to-container routing
.:53 {{
    log
    errors

    template IN A xmtpd.local {{
        answer "{{{{ .Name }}}} 60 IN A {traefik_ip}"
    }}

    # Forward everything else to Docker's internal DNS
    forward . 127.0.0.11

    cache 30
}}
"#
    )
}

/// Manages a CoreDNS Docker container for local DNS resolution.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
pub struct CoreDns {
    /// The CoreDNS image
    #[builder(default = CoreDnsConst::IMAGE.to_string())]
    image: String,

    /// The version tag
    #[builder(default = CoreDnsConst::VERSION.to_string())]
    version: String,

    /// Path to the Corefile configuration
    #[builder(default = "/tmp/xnet/coredns/Corefile".to_string())]
    corefile_path: String,

    /// Traefik container IP for container-facing DNS resolution
    traefik_ip: String,

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
        // Create Corefile with Traefik's IP for container-facing DNS
        if let Some(parent) = std::path::Path::new(&self.corefile_path).parent() {
            fs::create_dir_all(parent)?;
        }
        let corefile = generate_corefile(&self.traefik_ip);
        fs::write(&self.corefile_path, &corefile)?;
        info!(
            "Created Corefile at {} (traefik_ip={})",
            self.corefile_path, self.traefik_ip
        );

        let options = CreateContainerOptionsBuilder::default().name(CoreDnsConst::CONTAINER_NAME);

        // Port bindings:
        // - 5354: Host-facing DNS (resolves *.xmtpd.local to 127.0.0.1)
        // - 53: Container-facing DNS (rewrites to xnet-traefik), not exposed to host
        let port_bindings = hash_map! {
            "5354/udp".to_string() => expose_udp(CoreDnsConst::PORT),
            "5354/tcp".to_string() => expose(CoreDnsConst::PORT),
        };

        let config = ContainerCreateBody {
            image: Some(format!("{}:{}", self.image, self.version)),
            cmd: Some(vec![
                "-conf".to_string(),
                "/etc/coredns/Corefile".to_string(),
            ]),
            exposed_ports: Some(vec![
                "5354/udp".to_string(),
                "5354/tcp".to_string(),
                "53/udp".to_string(),
                "53/tcp".to_string(),
            ]),
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
            .start_container(CoreDnsConst::CONTAINER_NAME, options, config)
            .await?;

        info!(
            "CoreDNS started: port {} (host), port 53 (containers)",
            CoreDnsConst::PORT
        );
        Ok(())
    }

    /// Stop the CoreDNS container.
    pub async fn stop(&mut self) -> Result<()> {
        self.container
            .stop_container(CoreDnsConst::CONTAINER_NAME)
            .await
    }

    /// Check if CoreDNS is running.
    pub fn is_running(&self) -> bool {
        self.container.is_running()
    }

    /// Get the CoreDNS server address (for host queries).
    pub fn dns_server(&self) -> String {
        format!("127.0.0.1:{}", CoreDnsConst::PORT)
    }

    /// Get the CoreDNS container's IP address on the Docker network.
    /// This is used to configure other containers to use CoreDNS for DNS resolution.
    pub async fn container_ip(&self) -> Result<String> {
        self.container
            .container_ip(CoreDnsConst::CONTAINER_NAME, XNET_NETWORK_NAME)
            .await
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
        Url::parse(&format!("dns://127.0.0.1:{}", CoreDnsConst::PORT)).expect("valid URL")
    }

    fn external_url(&self) -> Url {
        self.url()
    }

    fn name(&self) -> String {
        "coredns".to_string()
    }

    fn port(&self) -> u16 {
        CoreDnsConst::PORT
    }

    // CoreDNS doesn't use ToxiProxy
    async fn register(&mut self, _toxiproxy: &ToxiProxy, _: Option<u16>) -> Result<u16> {
        Ok(CoreDnsConst::PORT)
    }
}
