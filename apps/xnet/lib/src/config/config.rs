//! Global Config

use alloy::signers::k256::SecretKey;
use alloy::{hex, signers::local::PrivateKeySigner};
use bon::Builder;
use color_eyre::eyre::{Result, eyre};
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::constants::{
    TOXIPROXY_STATIC_PORT_RANGE_END, TOXIPROXY_STATIC_PORT_RANGE_START,
    TOXIPROXY_XMTPD_PORT_RANGE_END, TOXIPROXY_XMTPD_PORT_RANGE_START,
};

use super::toml_config::{ImageConfig, MigrationConfig, NodeToml, TomlConfig};

static CONF: OnceLock<Config> = OnceLock::new();

#[derive(Builder, Debug, Clone)]
#[builder(on(String, into), derive(Debug))]
pub struct Config {
    /// use the same ports as in docker-compose.yml
    #[builder(default = true)]
    pub use_standard_ports: bool,
    /// Ethereum Signers for XMTPD
    pub signers: [PrivateKeySigner; 100],
    /// Static services ToxiProxy port range
    #[builder(default = (TOXIPROXY_STATIC_PORT_RANGE_START, TOXIPROXY_STATIC_PORT_RANGE_END))]
    pub port_range: (u16, u16),
    /// XMTPD nodes ToxiProxy port range
    #[builder(default = (TOXIPROXY_XMTPD_PORT_RANGE_START, TOXIPROXY_XMTPD_PORT_RANGE_END))]
    pub xmtpd_port_range: (u16, u16),
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
}

impl Config {
    const FILENAMES: &[&str] = &["xnet.toml", ".xnet.toml", ".xnet", ".config/xnet.toml"];

    /// Load config from TOML file (if found) merged with defaults.
    ///
    /// Searches these filenames: `xnet.toml`, `.xnet.toml`, `.xnet`, `.config/xnet.toml`
    /// in the following directories (short-circuits at first found):
    /// 1. Current directory
    /// 2. Git repository root
    /// 3. `$XDG_CONFIG_DIR/xnet` (Linux) or `~/Library/Application Support/xnet` (macOS)
    pub fn load() -> Result<Self> {
        if CONF.get().is_none() {
            let toml = Self::load_toml()?;
            let signers = Self::load_signers();
            let c = Config::builder()
                .use_standard_ports(toml.xnet.use_standard_ports.unwrap_or(true))
                .signers(signers)
                .port_range(
                    toml.xnet
                        .port_range
                        .map(|[s, e]| (s, e))
                        .unwrap_or((
                            TOXIPROXY_STATIC_PORT_RANGE_START,
                            TOXIPROXY_STATIC_PORT_RANGE_END,
                        )),
                )
                .xmtpd_port_range(
                    toml.xnet
                        .xmtpd_port_range
                        .map(|[s, e]| (s, e))
                        .unwrap_or((
                            TOXIPROXY_XMTPD_PORT_RANGE_START,
                            TOXIPROXY_XMTPD_PORT_RANGE_END,
                        )),
                )
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
                .build();
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

    fn find_config_file() -> Result<Option<PathBuf>> {
        for dir in Self::search_dirs()? {
            for name in Self::FILENAMES {
                let path = dir.join(name);
                if path.is_file() {
                    info!("found config: {}", path.display());
                    return Ok(Some(path));
                }
            }
        }
        Ok(None)
    }

    fn search_dirs() -> Result<Vec<PathBuf>> {
        let mut dirs = vec![std::env::current_dir()?];
        let sh = xshell::Shell::new()?;
        if let Ok(root) = xshell::cmd!(sh, "git rev-parse --show-toplevel").read() {
            let path = PathBuf::from(root.trim());
            if !dirs.contains(&path) {
                dirs.push(path);
            }
        }
        if let Some(proj) = directories::ProjectDirs::from("", "", "xnet") {
            dirs.push(proj.config_dir().to_path_buf());
        }
        Ok(dirs)
    }

    fn load_toml() -> Result<TomlConfig> {
        match Self::find_config_file()? {
            Some(path) => {
                let content = std::fs::read_to_string(&path)?;
                Ok(toml::from_str(&content)?)
            }
            None => Ok(TomlConfig::default()),
        }
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
