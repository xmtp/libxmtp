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
use super::toml_config::{ImageConfig, MigrationConfig, NodeToml, TomlConfig};

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
    /// Toxiproxy image overrides
    #[builder(default)]
    pub toxiproxy: ImageConfig,
    /// ToxiProxy port override
    pub toxiproxy_port: Option<u16>,
    /// Traefik HTTP host port override
    pub traefik_port: Option<u16>,
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
    /// Addressing mode (local or remote/sslip.io)
    #[builder(default)]
    pub address_mode: AddressMode,
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
            // Resolve address mode: XNET_REMOTE_IP env > --remote CLI flag > TOML remote_ip > Local
            let address_mode = if let Ok(env_ip) = std::env::var("XNET_REMOTE_IP") {
                match env_ip.parse::<std::net::IpAddr>() {
                    Ok(ip) => {
                        tracing::info!("Remote mode from env XNET_REMOTE_IP: {}", ip);
                        AddressMode::Remote(ip)
                    }
                    Err(_) => {
                        tracing::error!(
                            "XNET_REMOTE_IP is not a valid IP: '{}', falling back to local mode",
                            env_ip
                        );
                        AddressMode::Local
                    }
                }
            } else if let Some(ip) = app.args.remote {
                tracing::info!("Remote mode from --remote flag: {}", ip);
                AddressMode::Remote(ip)
            } else if let Some(ip) = toml.xnet.remote_ip {
                tracing::info!("Remote mode from TOML remote_ip: {}", ip);
                AddressMode::Remote(ip)
            } else {
                AddressMode::Local
            };

            let mut c = Config::builder()
                .use_standard_ports(toml.xnet.use_standard_ports)
                .paused(toml.xnet.paused)
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
                .toxiproxy(toml.toxiproxy)
                .maybe_toxiproxy_port(toml.xnet.toxiproxy_port)
                .maybe_traefik_port(toml.xnet.traefik_port)
                .prometheus(toml.prometheus)
                .grafana(toml.grafana)
                .address_mode(address_mode)
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
