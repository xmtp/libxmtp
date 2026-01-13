//! Prometheus metrics collection service.
//!
//! Runs a Prometheus Docker container that scrapes xmtpd nodes and the gateway.
//! Uses file-based service discovery so scrape targets update automatically
//! when nodes are added at runtime.
//! Uses direct port binding (no ToxiProxy).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use bollard::models::{ContainerCreateBody, HostConfig, PortBinding};
use bollard::query_parameters::CreateContainerOptionsBuilder;
use bon::Builder;
use color_eyre::eyre::Result;
use url::Url;

use crate::{
    constants::{Gateway as GatewayConst, Prometheus as PrometheusConst},
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy, Xmtpd},
};

/// Host directory for Prometheus configuration files.
const PROMETHEUS_CONFIG_DIR: &str = "/tmp/xnet/prometheus";
/// Host directory for Prometheus file_sd target files.
const PROMETHEUS_TARGETS_DIR: &str = "/tmp/xnet/prometheus/targets";

/// Manages a Prometheus Docker container with file-based service discovery.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
#[builder(finish_fn(vis = "", name = build_internal))]
pub struct Prometheus {
    /// The image name
    #[builder(default = PrometheusConst::IMAGE.to_string())]
    image: String,

    /// The version tag
    #[builder(default = PrometheusConst::VERSION.to_string())]
    version: String,

    /// Managed container state
    #[builder(default = ManagedContainer::new())]
    container: ManagedContainer,

    /// Path to the targets directory on the host
    #[builder(default = PathBuf::from(PROMETHEUS_TARGETS_DIR))]
    targets_dir: PathBuf,
}

impl<S: prometheus_builder::IsComplete> PrometheusBuilder<S> {
    pub fn build(self) -> Prometheus {
        let mut this = self.build_internal();
        if let Ok(config) = crate::Config::load() {
            if let Some(version) = config.prometheus.version {
                this.version = version;
            }
            if let Some(image) = config.prometheus.image {
                this.image = image;
            }
        }
        this
    }
}

impl Prometheus {
    /// Start the Prometheus container.
    ///
    /// Creates config files on the host and bind-mounts them into the container.
    /// Uses direct port binding (9090:9090) instead of ToxiProxy.
    pub async fn start(&mut self, _toxiproxy: &ToxiProxy) -> Result<()> {
        // Ensure config directories exist
        fs::create_dir_all(PROMETHEUS_CONFIG_DIR)?;
        fs::create_dir_all(PROMETHEUS_TARGETS_DIR)?;

        // Write the main prometheus.yml config
        let config_path = Path::new(PROMETHEUS_CONFIG_DIR).join("prometheus.yml");
        fs::write(
            &config_path,
            "global:\n  scrape_interval: 10s\n\nscrape_configs:\n  - job_name: xmtpd\n    file_sd_configs:\n      - files:\n          - /etc/prometheus/targets/*.json\n        refresh_interval: 5s\n",
        )?;

        // Write an initial empty targets file
        let targets_path = Path::new(PROMETHEUS_TARGETS_DIR).join("xmtpd.json");
        if !targets_path.exists() {
            fs::write(&targets_path, "[]")?;
        }

        let options =
            CreateContainerOptionsBuilder::default().name(PrometheusConst::CONTAINER_NAME);

        let image_ref = format!("{}:{}", self.image, self.version);

        let mut port_bindings = HashMap::new();
        port_bindings.insert(
            format!("{}/tcp", PrometheusConst::PORT),
            Some(vec![PortBinding {
                host_ip: Some("0.0.0.0".to_string()),
                host_port: Some(PrometheusConst::EXTERNAL_PORT.to_string()),
            }]),
        );

        let config = ContainerCreateBody {
            image: Some(image_ref),
            cmd: Some(vec![
                "--config.file=/etc/prometheus/prometheus.yml".to_string(),
                "--web.enable-lifecycle".to_string(),
                "--storage.tsdb.retention.time=1d".to_string(),
            ]),
            host_config: Some(HostConfig {
                network_mode: Some(XNET_NETWORK_NAME.to_string()),
                port_bindings: Some(port_bindings),
                binds: Some(vec![
                    format!(
                        "{}:/etc/prometheus/prometheus.yml:ro",
                        config_path.display()
                    ),
                    format!("{}:/etc/prometheus/targets:ro", PROMETHEUS_TARGETS_DIR),
                ]),
                ..Default::default()
            }),
            ..Default::default()
        };

        self.container
            .start_container(PrometheusConst::CONTAINER_NAME, options, config)
            .await?;

        Ok(())
    }

    /// Stop the Prometheus container.
    pub async fn stop(&mut self) -> Result<()> {
        self.container
            .stop_container(PrometheusConst::CONTAINER_NAME)
            .await
    }

    /// Update the Prometheus file_sd targets file with current xmtpd nodes.
    ///
    /// Prometheus watches this file via `file_sd_configs` and picks up changes
    /// automatically (within `refresh_interval`, default 5s).
    pub fn update_targets(&self, nodes: &[Xmtpd]) -> Result<()> {
        let targets_path = self.targets_dir.join("xmtpd.json");

        let mut groups = Vec::new();

        // Collect xmtpd node targets
        let node_targets: Vec<String> = nodes
            .iter()
            .map(|node| {
                format!(
                    "\"{}:{}\"",
                    node.container_name(),
                    PrometheusConst::METRICS_PORT
                )
            })
            .collect();

        if !node_targets.is_empty() {
            groups.push(format!(
                "  {{\"targets\": [{}], \"labels\": {{\"job\": \"xmtpd\"}}}}",
                node_targets.join(", ")
            ));
        }

        // Always include the gateway
        groups.push(format!(
            "  {{\"targets\": [\"{}:{}\"], \"labels\": {{\"job\": \"gateway\"}}}}",
            GatewayConst::CONTAINER_NAME,
            PrometheusConst::METRICS_PORT
        ));

        let json = format!("[\n{}\n]", groups.join(",\n"));
        fs::write(&targets_path, json)?;

        Ok(())
    }

    /// URL for use within the Docker network.
    pub fn url(&self) -> Url {
        Url::parse(&format!(
            "http://{}:{}",
            PrometheusConst::CONTAINER_NAME,
            PrometheusConst::PORT
        ))
        .expect("valid URL")
    }

    /// URL for external access (direct port binding).
    pub fn external_url(&self) -> Url {
        Url::parse(&format!(
            "http://localhost:{}",
            PrometheusConst::EXTERNAL_PORT
        ))
        .expect("valid URL")
    }

    /// Check if Prometheus is running.
    pub fn is_running(&self) -> bool {
        self.container.is_running()
    }
}

#[async_trait]
impl Service for Prometheus {
    async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        Prometheus::start(self, toxiproxy).await
    }

    async fn stop(&mut self) -> Result<()> {
        Prometheus::stop(self).await
    }

    fn is_running(&self) -> bool {
        Prometheus::is_running(self)
    }

    fn url(&self) -> Url {
        Prometheus::url(self)
    }

    fn external_url(&self) -> Url {
        Prometheus::external_url(self)
    }

    fn name(&self) -> String {
        "prometheus".to_string()
    }

    fn port(&self) -> u16 {
        PrometheusConst::EXTERNAL_PORT
    }

    /// No-op: Prometheus uses direct port binding, not ToxiProxy.
    async fn register(&mut self, _toxiproxy: &ToxiProxy, _: Option<u16>) -> Result<u16> {
        Ok(PrometheusConst::EXTERNAL_PORT)
    }
}
