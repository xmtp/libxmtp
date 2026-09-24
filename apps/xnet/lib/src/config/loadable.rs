//! Global Config

use alloy::primitives::Address;
use alloy::signers::k256::SecretKey;
use alloy::{hex, signers::local::PrivateKeySigner};
use bon::Builder;
use color_eyre::eyre::{Result, eyre};
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::app::App;
use crate::config::AppArgs;
use crate::constants::{ToxiProxy as ToxiProxyConst, Xmtpd as XmtpdConst};

use super::AddressMode;
use super::toml_config::{ExtraTraefikRoute, ImageConfig, MigrationConfig, NodeToml, TomlConfig};

static CONF: OnceLock<Config> = OnceLock::new();

/// Validate the node slice from TOML configuration.
///
/// Rules:
/// - At most one node may have `use_standard_port = true`
/// - A node cannot have both `use_standard_port = true` and an explicit `port`
pub fn validate_node_toml(nodes: &[NodeToml]) -> Result<()> {
    let standard_port_count = nodes.iter().filter(|n| n.use_standard_port).count();
    if standard_port_count > 1 {
        color_eyre::eyre::bail!(
            "at most one node may have `use_standard_port = true`, found {}",
            standard_port_count
        );
    }
    for node in nodes {
        if node.use_standard_port && node.port.is_some() {
            let name = node.name.as_deref().unwrap_or("<unnamed>");
            color_eyre::eyre::bail!(
                "node '{}': cannot set both `use_standard_port = true` and an explicit `port`",
                name
            );
        }
    }
    Ok(())
}

#[derive(Builder, Debug, Clone)]
#[builder(on(String, into), derive(Debug))]
pub struct Config {
    /// use the same ports as in docker-compose.yml
    #[builder(default = true)]
    pub use_standard_ports: bool,
    /// Pause broadcaster contracts on startup
    #[builder(default)]
    pub paused: bool,
    /// Ethereum Signers for XMTPD
    pub signers: [PrivateKeySigner; 100],
    /// Migration configuration
    #[builder(default)]
    pub migration: MigrationConfig,
    /// XMTPD image overrides
    #[builder(default)]
    pub xmtpd: ImageConfig,
    /// Path to an env file with extra XMTPD env vars
    pub xmtpd_env: Option<String>,
    /// Named XMTPD node definitions
    #[builder(default)]
    pub xmtpd_nodes: Vec<NodeToml>,
    /// V3 node-go image overrides
    #[builder(default)]
    pub v3: ImageConfig,
    /// V3 node-go port override
    pub v3_port: Option<u16>,
    /// Enable the V3 stack
    #[builder(default = true)]
    pub enable_v3: bool,
    /// Enable the D14n stack
    #[builder(default = true)]
    pub enable_d14n: bool,
    /// Enable monitoring services
    #[builder(default = true)]
    pub enable_monitoring: bool,
    /// Toxiproxy image overrides
    #[builder(default)]
    pub toxiproxy: ImageConfig,
    /// ToxiProxy port override
    pub toxiproxy_port: Option<u16>,
    /// Traefik image overrides
    #[builder(default)]
    pub traefik: ImageConfig,
    /// Traefik HTTP host port override
    pub traefik_port: Option<u16>,
    /// URL scheme for public references ("http" or "https").
    /// Set to "https" when TLS is terminated at CDN/proxy (e.g. Cloudflare).
    #[builder(default = "http".to_string())]
    pub public_scheme: String,
    /// Gateway image overrides
    #[builder(default)]
    pub gateway: ImageConfig,
    /// Validation service image overrides
    #[builder(default)]
    pub validation: ImageConfig,
    /// Contracts (anvil) image overrides
    #[builder(default)]
    pub contracts: ImageConfig,
    /// History server image overrides
    #[builder(default)]
    pub history: ImageConfig,
    /// Prometheus image overrides
    #[builder(default)]
    pub prometheus: ImageConfig,
    /// Grafana image overrides
    #[builder(default)]
    pub grafana: ImageConfig,
    /// Addressing mode (local or remote domain)
    #[builder(default)]
    pub address_mode: AddressMode,
    /// Extra Traefik routes from TOML config
    #[builder(default)]
    pub extra_traefik_routes: Vec<ExtraTraefikRoute>,
}

/// Validate that `remote_domain` is well-formed if provided.
pub fn validate_remote_domain(remote_domain: &Option<String>) -> Result<()> {
    if let Some(domain) = remote_domain {
        if domain.is_empty() {
            color_eyre::eyre::bail!("`remote_domain` must not be empty");
        }
        if domain.starts_with('.') || domain.ends_with('.') {
            color_eyre::eyre::bail!("`remote_domain` must not start or end with '.'");
        }
    }
    Ok(())
}

impl Config {
    /// Load config from TOML file (if found) merged with defaults.
    ///
    /// Searches these filenames: `xnet.toml`, `.xnet.toml`, `.xnet`, `.config/xnet.toml`
    /// in the following directories (short-circuits at first found):
    /// 1. Current directory
    /// 2. Git repository root
    /// 3. `$XDG_CONFIG_DIR/xnet` (Linux) or `~/Library/Application Support/xnet` (macOS)
    pub fn load() -> Result<Self> {
        if CONF.get().is_none() {
            let app = App::parse()?;
            let toml = Self::load_toml(&app.args)?;
            let signers = Self::load_signers();
            // Resolve address mode: env > CLI > TOML
            let effective_remote_domain = std::env::var("XNET_REMOTE_DOMAIN")
                .ok()
                .filter(|s| !s.is_empty())
                .or(app.args.remote_domain.clone())
                .or(toml.xnet.remote_domain.clone());

            validate_remote_domain(&effective_remote_domain)?;

            let address_mode = if let Some(domain) = effective_remote_domain {
                tracing::info!("Remote domain mode: {}", domain);
                AddressMode::RemoteDomain(domain)
            } else {
                AddressMode::Local
            };

            // Merge CLI --paused flag with TOML paused setting
            let cli_paused = matches!(
                app.args.cmd,
                Some(crate::config::Commands::Up(crate::config::Up {
                    paused: true
                }))
            );
            let paused = cli_paused || toml.xnet.paused;

            let mut c = Config::builder()
                .use_standard_ports(toml.xnet.use_standard_ports)
                .paused(paused)
                .signers(signers)
                .migration(toml.migration)
                .xmtpd(toml.xmtpd.image)
                .maybe_xmtpd_env(toml.xmtpd.env)
                .xmtpd_nodes(toml.xmtpd.nodes)
                .v3(toml.v3.image)
                .maybe_v3_port(toml.v3.port)
                .gateway(toml.gateway)
                .validation(toml.validation)
                .contracts(toml.contracts)
                .history(toml.history)
                .toxiproxy(toml.toxiproxy.image)
                .maybe_toxiproxy_port(toml.toxiproxy.port)
                .traefik(toml.traefik.image)
                .maybe_traefik_port(toml.traefik.port)
                .public_scheme(
                    toml.xnet
                        .public_scheme
                        .clone()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "http".to_string()),
                )
                .enable_v3(toml.xnet.enable_v3)
                .enable_d14n(toml.xnet.enable_d14n)
                .enable_monitoring(toml.xnet.enable_monitoring)
                .prometheus(toml.prometheus)
                .grafana(toml.grafana)
                .address_mode(address_mode)
                .extra_traefik_routes(toml.extra_traefik_routes)
                .build();

            // Allow XNET_CUTOVER_TIMESTAMP env var to override the TOML migration_timestamp
            if let Ok(env_val) = std::env::var("XNET_CUTOVER_TIMESTAMP") {
                match env_val.parse::<u64>() {
                    Ok(ts) => {
                        tracing::info!(
                            "Overriding migration_timestamp from env: {} (config had: {:?})",
                            ts,
                            c.migration.migration_timestamp
                        );
                        c.migration.migration_timestamp = Some(ts);
                    }
                    Err(_) => {
                        tracing::warn!(
                            "XNET_CUTOVER_TIMESTAMP env var is not a valid u64: '{}', falling back to TOML value",
                            env_val
                        );
                    }
                }
            }

            // Validate node configuration
            validate_node_toml(&c.xmtpd_nodes)?;

            CONF.set(c)
                .map_err(|_| eyre!("Config already initialized"))?;
        }
        CONF.get()
            .cloned()
            .ok_or_else(|| eyre!("Config not initialized"))
    }

    pub fn load_unchecked() -> Self {
        CONF.get()
            .expect("config loaded without checking if exists")
            .clone()
    }

    fn load_toml(args: &AppArgs) -> Result<TomlConfig> {
        match &args.config {
            Some(path) => {
                let content = std::fs::read_to_string(path)?;
                Ok(toml::from_str(&content)?)
            }
            None => Ok(TomlConfig::default()),
        }
    }

    /// Derive the Ethereum address for an xmtpd node from its ID.
    ///
    /// Node IDs are assigned in increments of `NODE_ID_INCREMENT` (100).
    /// Each node uses 3 consecutive signers starting at index `(node_id / 100) * 3 + 1`.
    /// The first signer (at the base index) is the node's signing key.
    pub fn address_for_node(&self, node_id: u32) -> Address {
        let idx = (node_id / XmtpdConst::NODE_ID_INCREMENT) as usize * 3 + 1;
        self.signers[idx].address()
    }

    pub fn payer_address_for_node(&self, node_id: u32) -> Address {
        let idx = (node_id / XmtpdConst::NODE_ID_INCREMENT) as usize * 3 + 2;
        self.signers[idx].address()
    }

    pub fn migration_payer_address_for_node(&self, node_id: u32) -> Address {
        let idx = (node_id / XmtpdConst::NODE_ID_INCREMENT) as usize * 3 + 3;
        self.signers[idx].address()
    }

    fn load_signers() -> [PrivateKeySigner; 100] {
        let signers: &'static str = include_str!("./../../signers.txt");
        let signers: Vec<_> = signers
            .trim()
            .split('\n')
            .map(|s| hex::decode(s).expect("static signer must be valid"))
            .map(|b| PrivateKeySigner::from_slice(&b).expect("static signer must be correct"))
            .collect();
        signers.try_into().expect("constant file must convert")
    }
}
