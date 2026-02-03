//! TOML configuration file schema

use serde::Deserialize;

/// Reusable image+version pair for any Docker service
#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct ImageConfig {
    pub image: Option<String>,
    pub version: Option<String>,
}

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
}

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct XnetToml {
    pub use_standard_ports: Option<bool>,
    pub port_range: Option<[u16; 2]>,
    pub xmtpd_port_range: Option<[u16; 2]>,
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
    pub name: Option<String>,
    pub port: Option<u16>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct V3Toml {
    #[serde(flatten)]
    pub image: ImageConfig,
    pub port: Option<u16>,
}
