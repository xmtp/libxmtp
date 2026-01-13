//! Global Config

use alloy::signers::k256::SecretKey;
use alloy::{hex, signers::local::PrivateKeySigner};
use bon::Builder;
use color_eyre::eyre::{Result, eyre};
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::app::App;
use crate::config::AppArgs;
use crate::constants::ToxiProxy as ToxiProxyConst;

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
            let c = Config::builder()
                .use_standard_ports(toml.xnet.use_standard_ports)
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

    fn load_toml(args: &AppArgs) -> Result<TomlConfig> {
        match &args.config {
            Some(path) => {
                let content = std::fs::read_to_string(path)?;
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
