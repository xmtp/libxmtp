//! Grafana dashboard service.
//!
//! Runs the `ghcr.io/xmtp/grafana-xmtpd` Docker container which has
//! pre-built xmtpd dashboards baked in. Auto-provisions a Prometheus
//! data source pointing at the xnet-prometheus container.
//! Uses direct port binding (no ToxiProxy).

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use async_trait::async_trait;
use bollard::models::{ContainerCreateBody, HostConfig, PortBinding};
use bollard::query_parameters::CreateContainerOptionsBuilder;
use bon::Builder;
use color_eyre::eyre::Result;
use url::Url;

use crate::{
    constants::{Grafana as GrafanaConst, Prometheus as PrometheusConst},
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy},
};

/// Host directory for Grafana datasource provisioning file.
const GRAFANA_DATASOURCES_DIR: &str = "/tmp/xnet/grafana/datasources";
/// Host directory mounted over baked-in alerting provisioning to suppress
/// production contact-point configs that reference unconfigured Slack webhooks.
const GRAFANA_ALERTING_DIR: &str = "/tmp/xnet/grafana/alerting";

/// Manages a Grafana Docker container with pre-built xmtpd dashboards.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
#[builder(finish_fn(vis = "", name = build_internal))]
pub struct Grafana {
    /// The image name
    #[builder(default = GrafanaConst::IMAGE.to_string())]
    image: String,

    /// The version tag
    #[builder(default = GrafanaConst::VERSION.to_string())]
    version: String,

    /// Managed container state
    #[builder(default = ManagedContainer::new())]
    container: ManagedContainer,
}

impl<S: grafana_builder::IsComplete> GrafanaBuilder<S> {
    pub fn build(self) -> Grafana {
        let mut this = self.build_internal();
        if let Ok(config) = crate::Config::load() {
            if let Some(version) = config.grafana.version {
                this.version = version;
            }
            if let Some(image) = config.grafana.image {
                this.image = image;
            }
        }
        this
    }
}

impl Grafana {
    /// Start the Grafana container.
    ///
    /// Writes a datasource provisioning file that points at the Prometheus
    /// container, then starts Grafana with direct port binding (3000:3000).
    pub async fn start(&mut self, _toxiproxy: &ToxiProxy) -> Result<()> {
        // Write a datasource provisioning file on the host that we'll
        // bind-mount over *only* the datasources directory.  This preserves
        // the dashboards, alerting rules and other provisioning files that
        // are baked into the grafana-xmtpd image.
        fs::create_dir_all(GRAFANA_DATASOURCES_DIR)?;
        // Create an empty alerting directory to shadow the baked-in production
        // alerting provisioning (contactpoints.yaml references a Slack webhook
        // that isn't configured locally, which causes Grafana to crash).
        fs::create_dir_all(GRAFANA_ALERTING_DIR)?;

        // Provision multiple datasource entries that all point at the same
        // local Prometheus but with different names/UIDs.  The baked-in
        // dashboards use template variables that filter datasources by regex:
        //   - `/.*Node.*/`  → needs a name containing "Node"
        //   - `/.*[Pp]ayer.*/` → needs a name containing "Payer"
        // Some panels also hard-code UIDs like `amp_node100` or `amp_gateway`.
        let prom_url = format!(
            "http://{}:{}",
            PrometheusConst::CONTAINER_NAME,
            PrometheusConst::PORT
        );
        let datasource_path = Path::new(GRAFANA_DATASOURCES_DIR).join("datasources.yaml");
        fs::write(
            &datasource_path,
            format!(
                "\
apiVersion: 1
datasources:
  - name: Prometheus
    uid: amp-prom
    type: prometheus
    access: proxy
    url: {prom_url}
    isDefault: true
    editable: false
    jsonData:
      httpMethod: POST
      timeInterval: 15s
  - name: AMP - XMTPD Node
    uid: amp_node100
    type: prometheus
    access: proxy
    url: {prom_url}
    editable: false
    jsonData:
      httpMethod: POST
      timeInterval: 15s
  - name: AMP - Gateway Payer
    uid: amp_gateway
    type: prometheus
    access: proxy
    url: {prom_url}
    editable: false
    jsonData:
      httpMethod: POST
      timeInterval: 15s
"
            ),
        )?;

        let options = CreateContainerOptionsBuilder::default().name(GrafanaConst::CONTAINER_NAME);

        let image_ref = format!("{}:{}", self.image, self.version);

        let mut port_bindings = HashMap::new();
        port_bindings.insert(
            format!("{}/tcp", GrafanaConst::PORT),
            Some(vec![PortBinding {
                host_ip: Some("0.0.0.0".to_string()),
                host_port: Some(GrafanaConst::EXTERNAL_PORT.to_string()),
            }]),
        );

        let config = ContainerCreateBody {
            image: Some(image_ref),
            env: Some(vec![
                "GF_AUTH_ANONYMOUS_ENABLED=true".to_string(),
                "GF_AUTH_ANONYMOUS_ORG_NAME=Main Org.".to_string(),
                "GF_AUTH_ANONYMOUS_ORG_ROLE=Viewer".to_string(),
                "GF_SECURITY_ADMIN_PASSWORD=admin".to_string(),
            ]),
            host_config: Some(HostConfig {
                network_mode: Some(XNET_NETWORK_NAME.to_string()),
                port_bindings: Some(port_bindings),
                // Mount our datasources config and an empty alerting dir.
                // The empty alerting dir shadows baked-in production alerting
                // configs that reference unconfigured Slack webhooks.
                // Dashboards provisioning remains intact from the image.
                binds: Some(vec![
                    format!(
                        "{}:/etc/grafana/provisioning/datasources",
                        GRAFANA_DATASOURCES_DIR
                    ),
                    format!(
                        "{}:/etc/grafana/provisioning/alerting",
                        GRAFANA_ALERTING_DIR
                    ),
                ]),
                ..Default::default()
            }),
            ..Default::default()
        };

        self.container
            .start_container(GrafanaConst::CONTAINER_NAME, options, config)
            .await?;

        Ok(())
    }

    /// Stop the Grafana container.
    pub async fn stop(&mut self) -> Result<()> {
        self.container
            .stop_container(GrafanaConst::CONTAINER_NAME)
            .await
    }

    /// URL for use within the Docker network.
    pub fn url(&self) -> Url {
        Url::parse(&format!(
            "http://{}:{}",
            GrafanaConst::CONTAINER_NAME,
            GrafanaConst::PORT
        ))
        .expect("valid URL")
    }

    /// URL for external access (direct port binding).
    pub fn external_url(&self) -> Url {
        Url::parse(&format!("http://localhost:{}", GrafanaConst::EXTERNAL_PORT)).expect("valid URL")
    }

    /// Check if Grafana is running.
    pub fn is_running(&self) -> bool {
        self.container.is_running()
    }
}

#[async_trait]
impl Service for Grafana {
    async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        Grafana::start(self, toxiproxy).await
    }

    async fn stop(&mut self) -> Result<()> {
        Grafana::stop(self).await
    }

    fn is_running(&self) -> bool {
        Grafana::is_running(self)
    }

    fn url(&self) -> Url {
        Grafana::url(self)
    }

    fn external_url(&self) -> Url {
        Grafana::external_url(self)
    }

    fn name(&self) -> String {
        "grafana".to_string()
    }

    fn port(&self) -> u16 {
        GrafanaConst::EXTERNAL_PORT
    }

    /// No-op: Grafana uses direct port binding, not ToxiProxy.
    async fn register(&mut self, _toxiproxy: &ToxiProxy, _: Option<u16>) -> Result<u16> {
        Ok(GrafanaConst::EXTERNAL_PORT)
    }
}
