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
    pub toxiproxy: ToxiProxyToml,
    pub prometheus: ImageConfig,
    pub grafana: ImageConfig,
    pub traefik: TraefikToml,
    #[serde(default)]
    pub extra_traefik_routes: Vec<ExtraTraefikRoute>,
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
    /// Pause broadcaster contracts on startup (same as `--paused` CLI flag)
    pub paused: bool,
    /// Custom domain for remote addressing mode.
    /// Hostnames become {name}.{domain} instead of {name}.xmtpd.local.
    pub remote_domain: Option<String>,
    /// Enable the V3 stack (Validation, V3Db, MlsDb, History, NodeGo)
    pub enable_v3: bool,
    /// Enable the D14n stack (Redis, Gateway, XMTPD nodes)
    pub enable_d14n: bool,
    /// Enable monitoring services (Prometheus, Grafana, PgAdmin, Otterscan)
    pub enable_monitoring: bool,
    /// URL scheme for external references (gateway responses, node URLs).
    /// "http" or "https". Default: "http".
    /// Set to "https" when TLS is terminated at a CDN/proxy (e.g. Cloudflare).
    pub public_scheme: Option<String>,
}

impl Default for XnetToml {
    fn default() -> Self {
        Self {
            use_standard_ports: true,
            paused: false,
            remote_domain: None,
            enable_v3: true,
            enable_d14n: true,
            enable_monitoring: true,
            public_scheme: None,
        }
    }
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct TraefikToml {
    #[serde(flatten)]
    pub image: ImageConfig,
    pub port: Option<u16>,
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct ToxiProxyToml {
    #[serde(flatten)]
    pub image: ImageConfig,
    pub port: Option<u16>,
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
    pub use_standard_port: bool,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct V3Toml {
    #[serde(flatten)]
    pub image: ImageConfig,
    pub port: Option<u16>,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct ExtraTraefikRoute {
    pub name: String,
    pub rule: String,
    pub url: String,
    pub priority: Option<i32>,
}
