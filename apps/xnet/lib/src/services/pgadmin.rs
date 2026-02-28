//! PgAdmin 4 web UI for browsing PostgreSQL databases.
//!
//! Runs the `dpage/pgadmin4` Docker container with a pre-configured `servers.json`
//! that lists all known databases on the xnet network. Uses direct port binding
//! (no ToxiProxy).

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
    constants::{
        MlsDb as MlsDbConst, PgAdmin as PgAdminConst, ReplicationDb as ReplicationDbConst,
        V3Db as V3DbConst,
    },
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy, Xmtpd},
};

/// Host directory for PgAdmin configuration files.
const PGADMIN_DIR: &str = "/tmp/xnet/pgadmin";

/// Manages a PgAdmin 4 Docker container.
#[derive(Builder)]
#[builder(on(String, into), derive(Debug))]
pub struct PgAdmin {
    /// The image name
    #[builder(default = PgAdminConst::IMAGE.to_string())]
    image: String,

    /// The version tag
    #[builder(default = PgAdminConst::VERSION.to_string())]
    version: String,

    /// Managed container state
    #[builder(default = ManagedContainer::new())]
    container: ManagedContainer,
}

impl PgAdmin {
    /// Start the PgAdmin container.
    ///
    /// Writes a `servers.json` with the static databases, then starts
    /// PgAdmin with direct port binding (5600:80).
    pub async fn start(&mut self, _toxiproxy: &ToxiProxy) -> Result<()> {
        fs::create_dir_all(PGADMIN_DIR)?;

        // Write initial servers.json with static databases
        self.write_servers(&[])?;

        let options = CreateContainerOptionsBuilder::default().name(PgAdminConst::CONTAINER_NAME);

        let image_ref = format!("{}:{}", self.image, self.version);

        let mut port_bindings = HashMap::new();
        port_bindings.insert(
            format!("{}/tcp", PgAdminConst::PORT),
            Some(vec![PortBinding {
                host_ip: Some("0.0.0.0".to_string()),
                host_port: Some(PgAdminConst::EXTERNAL_PORT.to_string()),
            }]),
        );

        let config = ContainerCreateBody {
            image: Some(image_ref),
            env: Some(vec![
                "PGADMIN_DEFAULT_EMAIL=admin@xnet.dev".to_string(),
                "PGADMIN_DEFAULT_PASSWORD=admin".to_string(),
                "PGADMIN_CONFIG_SERVER_MODE=False".to_string(),
                "PGADMIN_CONFIG_MASTER_PASSWORD_REQUIRED=False".to_string(),
                "PGADMIN_CONFIG_CHECK_EMAIL_DELIVERABILITY=False".to_string(),
            ]),
            host_config: Some(HostConfig {
                network_mode: Some(XNET_NETWORK_NAME.to_string()),
                port_bindings: Some(port_bindings),
                binds: Some(vec![format!(
                    "{}/servers.json:/pgadmin4/servers.json",
                    PGADMIN_DIR
                )]),
                ..Default::default()
            }),
            ..Default::default()
        };

        self.container
            .start_container(PgAdminConst::CONTAINER_NAME, options, config)
            .await?;

        Ok(())
    }

    /// Stop the PgAdmin container.
    pub async fn stop(&mut self) -> Result<()> {
        self.container
            .stop_container(PgAdminConst::CONTAINER_NAME)
            .await
    }

    /// Regenerate `servers.json` with static DBs plus any xmtpd replication DBs.
    ///
    /// PgAdmin reads this file at startup. If the container is already running,
    /// changes take effect on the next restart.
    pub fn update_servers(&self, nodes: &[Xmtpd]) -> Result<()> {
        self.write_servers(nodes)
    }

    /// Write the servers.json file.
    fn write_servers(&self, nodes: &[Xmtpd]) -> Result<()> {
        let servers_path = Path::new(PGADMIN_DIR).join("servers.json");

        let mut server_id = 0u32;
        let mut entries = Vec::new();

        // Static databases
        server_id += 1;
        entries.push(format!(
            r#"    "{}": {{
      "Name": "xnet-db (V3)",
      "Group": "XNET",
      "Host": "{}",
      "Port": {},
      "MaintenanceDB": "postgres",
      "Username": "postgres",
      "SSLMode": "prefer"
    }}"#,
            server_id,
            V3DbConst::CONTAINER_NAME,
            5432
        ));

        server_id += 1;
        entries.push(format!(
            r#"    "{}": {{
      "Name": "xnet-mlsdb (MLS)",
      "Group": "XNET",
      "Host": "{}",
      "Port": {},
      "MaintenanceDB": "postgres",
      "Username": "postgres",
      "SSLMode": "prefer"
    }}"#,
            server_id,
            MlsDbConst::CONTAINER_NAME,
            5432
        ));

        // Dynamic replication databases (one per xmtpd node)
        for node in nodes {
            server_id += 1;
            let db_name = format!(
                "xmtpd-db-{}",
                node.container_name().trim_start_matches("xnet-")
            );
            entries.push(format!(
                r#"    "{}": {{
      "Name": "{} (Replication)",
      "Group": "XNET",
      "Host": "{}",
      "Port": {},
      "MaintenanceDB": "postgres",
      "Username": "postgres",
      "SSLMode": "prefer"
    }}"#,
                server_id, db_name, db_name, 5432
            ));
        }

        let json = format!("{{\n  \"Servers\": {{\n{}\n  }}\n}}", entries.join(",\n"));
        fs::write(&servers_path, json)?;

        Ok(())
    }

    /// URL for use within the Docker network.
    pub fn url(&self) -> Url {
        Url::parse(&format!(
            "http://{}:{}",
            PgAdminConst::CONTAINER_NAME,
            PgAdminConst::PORT
        ))
        .expect("valid URL")
    }

    /// URL for external access (direct port binding).
    pub fn external_url(&self) -> Url {
        Url::parse(&format!("http://localhost:{}", PgAdminConst::EXTERNAL_PORT)).expect("valid URL")
    }

    /// Check if PgAdmin is running.
    pub fn is_running(&self) -> bool {
        self.container.is_running()
    }
}

#[async_trait]
impl Service for PgAdmin {
    async fn start(&mut self, toxiproxy: &ToxiProxy) -> Result<()> {
        PgAdmin::start(self, toxiproxy).await
    }

    async fn stop(&mut self) -> Result<()> {
        PgAdmin::stop(self).await
    }

    fn is_running(&self) -> bool {
        PgAdmin::is_running(self)
    }

    fn url(&self) -> Url {
        PgAdmin::url(self)
    }

    fn external_url(&self) -> Url {
        PgAdmin::external_url(self)
    }

    fn name(&self) -> String {
        "pgadmin".to_string()
    }

    fn port(&self) -> u16 {
        PgAdminConst::EXTERNAL_PORT
    }

    /// No-op: PgAdmin uses direct port binding, not ToxiProxy.
    async fn register(&mut self, _toxiproxy: &ToxiProxy, _: Option<u16>) -> Result<u16> {
        Ok(PgAdminConst::EXTERNAL_PORT)
    }
}
