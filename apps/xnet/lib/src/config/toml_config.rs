//! TOML configuration file schema

use serde::Deserialize;
/// Raw TOML file representation — all fields optional
#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub(crate) struct TomlConfig {
    pub xnet: XnetToml,
    pub migration: MigrationConfig,
    pub xmtpd: XmtpdToml,
    pub v3: V3Toml,
    pub gateway: ImageConfig,
    pub validation: ImageConfig,
    pub contracts: ImageConfig,
    pub history: ImageConfig,
    pub toxiproxy: ImageConfig,
    pub prometheus: ImageConfig,
    pub grafana: ImageConfig,
}

/// Reusable image+version pair for any Docker service
#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct ImageConfig {
    pub image: Option<String>,
    pub version: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct XnetToml {
    pub use_standard_ports: bool,
    pub toxiproxy_port: Option<u16>,
    /// Override the Traefik HTTP host port (default: 80)
    pub traefik_port: Option<u16>,
    /// Pause broadcaster contracts on startup (same as `--paused` CLI flag)
    pub paused: bool,
    /// Public IP for remote addressing mode (sslip.io).
    /// Equivalent to the --remote CLI flag.
    pub remote_ip: Option<std::net::IpAddr>,
}

impl Default for XnetToml {
    fn default() -> Self {
        Self {
            use_standard_ports: true,
            toxiproxy_port: Default::default(),
            traefik_port: None,
            paused: false,
            remote_ip: None,
        }
    }
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct MigrationConfig {
    pub enable: bool,
    pub migration_timestamp: Option<u64>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct XmtpdToml {
    #[serde(flatten)]
    pub image: ImageConfig,
    pub env: Option<String>,
    pub nodes: Vec<NodeToml>,
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct NodeToml {
    pub enable: bool,
    pub name: Option<String>,
    pub port: Option<u16>,
    pub migrator: bool,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct V3Toml {
    #[serde(flatten)]
    pub image: ImageConfig,
    pub port: Option<u16>,
}
