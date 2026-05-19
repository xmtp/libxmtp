//! PgAdmin 4 web UI for browsing PostgreSQL databases.
//!
//! Runs the `dpage/pgadmin4` Docker container with a pre-configured `servers.json`
//! that lists all known databases on the xnet network.
//!
//! ## Database Discovery
//!
//! PgAdmin discovers databases in two ways:
//! - **Static databases**: `xnet-db` (V3) and `xnet-mlsdb` (MLS) are always included.
//! - **Dynamic databases**: Any Docker container with the label `xnet.pgadmin=true`
//!   is automatically added. The container must also have labels:
//!   - `xnet.pgadmin.name` — display name in PgAdmin UI
//!   - `xnet.pgadmin.host` — Docker network hostname
//!   - `xnet.pgadmin.port` — database port
//!
//! ## Dependency Chain
//!
//! PgAdmin must start AFTER any services whose databases should appear at startup.
//! Currently this means xmtpd nodes (which create labeled ReplicationDb containers).
//! See `ServiceManager::start()` for the ordering.
//! For databases added at runtime, call `discover_databases()` to rescan.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use async_trait::async_trait;
use bollard::Docker;
use bollard::models::{ContainerCreateBody, HostConfig, PortBinding};
use bollard::query_parameters::{CreateContainerOptionsBuilder, ListContainersOptionsBuilder};
use bon::Builder;
use color_eyre::eyre::Result;
use serde::Serialize;
use url::Url;

use crate::{
    constants::{MlsDb as MlsDbConst, PgAdmin as PgAdminConst, V3Db as V3DbConst},
    network::XNET_NETWORK_NAME,
    services::{ManagedContainer, Service, ToxiProxy},
};

/// Host directory for PgAdmin configuration files.
const PGADMIN_DIR: &str = "/tmp/xnet/pgadmin";

/// Docker label that marks a container as a PgAdmin-discoverable database.
const PGADMIN_LABEL: &str = "xnet.pgadmin";

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
    /// Scans Docker for labeled database containers and writes `servers.json`,
    /// then starts PgAdmin with direct port binding (5600:80).
    ///
    /// **Dependency:** Must be called AFTER xmtpd nodes have started, so their
    /// ReplicationDb containers (with `xnet.pgadmin=true` labels) exist to scan.
    pub async fn start(&mut self, _toxiproxy: &ToxiProxy) -> Result<()> {
        fs::create_dir_all(PGADMIN_DIR)?;

        // Discover labeled database containers and write servers.json
        self.discover_databases().await?;

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

    /// Discover databases by scanning Docker for containers with `xnet.pgadmin=true`.
    ///
    /// Regenerates `servers.json` with static databases plus any discovered containers.
    /// Call this after adding new xmtpd nodes at runtime.
    pub async fn discover_databases(&self) -> Result<()> {
        let docker = Docker::connect_with_socket_defaults()?;

        // Query Docker for containers on the xnet network with the pgadmin label
        let mut filters = HashMap::new();
        filters.insert("label".to_string(), vec![format!("{}=true", PGADMIN_LABEL)]);
        filters.insert("network".to_string(), vec![XNET_NETWORK_NAME.to_string()]);

        let options = ListContainersOptionsBuilder::default()
            .filters(&filters)
            .build();
        let containers = docker.list_containers(Some(options)).await?;

        // Extract database entries from container labels
        let mut discovered = Vec::new();
        for container in &containers {
            let labels = match &container.labels {
                Some(l) => l,
                None => continue,
            };

            let name = match labels.get("xnet.pgadmin.name") {
                Some(n) => n.clone(),
                None => continue,
            };
            let host = match labels.get("xnet.pgadmin.host") {
                Some(h) => h.clone(),
                None => continue,
            };
            let port: u16 = match labels.get("xnet.pgadmin.port") {
                Some(p) => p.parse().unwrap_or(5432),
                None => 5432,
            };

            discovered.push(DiscoveredDb { name, host, port });
        }

        self.write_servers(&discovered)?;
        Ok(())
    }

    /// Write the servers.json file with static databases and discovered databases.
    fn write_servers(&self, discovered: &[DiscoveredDb]) -> Result<()> {
        fs::create_dir_all(PGADMIN_DIR)?;
        let servers_path = Path::new(PGADMIN_DIR).join("servers.json");

        let mut server_id = 0u32;
        let mut servers = HashMap::new();

        // Static databases (always present)
        server_id += 1;
        servers.insert(
            server_id.to_string(),
            PgAdminServer {
                name: "xnet-db (V3)".to_string(),
                group: "XNET".to_string(),
                host: V3DbConst::CONTAINER_NAME.to_string(),
                port: 5432,
                maintenance_db: "postgres".to_string(),
                username: "postgres".to_string(),
                ssl_mode: "prefer".to_string(),
            },
        );

        server_id += 1;
        servers.insert(
            server_id.to_string(),
            PgAdminServer {
                name: "xnet-mlsdb (MLS)".to_string(),
                group: "XNET".to_string(),
                host: MlsDbConst::CONTAINER_NAME.to_string(),
                port: 5432,
                maintenance_db: "postgres".to_string(),
                username: "postgres".to_string(),
                ssl_mode: "disable".to_string(),
            },
        );

        // Discovered databases (from Docker labels)
        for db in discovered {
            server_id += 1;
            servers.insert(
                server_id.to_string(),
                PgAdminServer {
                    name: db.name.clone(),
                    group: "XNET".to_string(),
                    host: db.host.clone(),
                    port: db.port,
                    maintenance_db: "postgres".to_string(),
                    username: "postgres".to_string(),
                    ssl_mode: "disable".to_string(),
                },
            );
        }

        let wrapper = PgAdminServersFile { servers };
        let json = serde_json::to_string_pretty(&wrapper)?;
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

/// A database discovered from Docker container labels.
struct DiscoveredDb {
    /// Display name for PgAdmin UI (from `xnet.pgadmin.name` label)
    name: String,
    /// Docker network hostname (from `xnet.pgadmin.host` label)
    host: String,
    /// Database port (from `xnet.pgadmin.port` label)
    port: u16,
}

/// Top-level structure for PgAdmin's `servers.json` file.
#[derive(Serialize)]
struct PgAdminServersFile {
    #[serde(rename = "Servers")]
    servers: HashMap<String, PgAdminServer>,
}

/// A single server entry in PgAdmin's `servers.json`.
#[derive(Serialize)]
struct PgAdminServer {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Group")]
    group: String,
    #[serde(rename = "Host")]
    host: String,
    #[serde(rename = "Port")]
    port: u16,
    #[serde(rename = "MaintenanceDB")]
    maintenance_db: String,
    #[serde(rename = "Username")]
    username: String,
    #[serde(rename = "SSLMode")]
    ssl_mode: String,
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
