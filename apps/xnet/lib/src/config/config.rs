//! Global Config

use alloy::signers::k256::SecretKey;
use alloy::{hex, signers::local::PrivateKeySigner};
use bon::Builder;
use color_eyre::eyre::Result;
use std::sync::OnceLock;

static CONF: OnceLock<Config> = OnceLock::new();

#[derive(Builder, Debug, Clone)]
#[builder(on(String, into), derive(Debug))]
pub struct Config {
    /// use the same ports as in docker-compose.yml
    #[builder(default = true)]
    pub use_standard_ports: bool,
    /// Ethereum Signers for XMTPD
    pub signers: [PrivateKeySigner; 100],
}

impl Config {
    /// Load config from the environment if it exists
    /// Checks these filenames:
    /// - xnet.toml
    /// - .xnet.toml
    /// - .xnet
    /// - .config/xnet.toml
    /// In the following order of directories, stops at the first configuration found
    /// (short-circuits):
    /// - Current Directory
    /// - Current Git Root
    /// - $XDG_CONFIG_DIR on Linux or $HOME/Library/Application Support on Darwin
    pub fn load() -> Result<Self> {
        if CONF.get().is_none() {
            // load from toml
            let signers = Self::load_signers();
            let c = Config::builder().signers(signers).build();
            CONF.set(c).expect("Must not be initialized");
        }
        let c = CONF.get().expect("Must already be set");
        Ok(c.clone())
    }

    pub fn load_unchecked() -> Self {
        CONF.get()
            .expect("config loaded without checking if exists")
            .clone()
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
