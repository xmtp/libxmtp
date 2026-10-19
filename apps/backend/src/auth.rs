//! API key and JWT verification with bounded signing-key refresh.
pub(crate) mod jwks;
pub(crate) mod keys;
pub(crate) mod verify;

pub use verify::AuthContext;

use crate::config::auth::SigningAlgorithm;

pub(crate) struct Authentication {
    pub verifier: std::sync::Arc<verify::Verifier>,
    pub jwks: Option<jwks::JwksSource>,
    pub last_success: xmtp_common::time::Instant,
}

impl Authentication {
    /// Load keys before any listener binds. Startup fetch errors contain only the host.
    /// An inline key that does not parse for its algorithm returns
    /// `ConfigError::Auth` naming its key under `auth.keys[N]`, without the
    /// key material.
    pub async fn initialize(
        config: &crate::config::auth::AuthConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let jwks = config
            .jwks_url
            .as_ref()
            .map(|url| jwks::JwksSource::new(url, config.clone()))
            .transpose()?;
        let loaded = match &jwks {
            Some(source) => source.startup().await?,
            None => keys::inline(config)?,
        };
        let keys = std::sync::Arc::new(keys::KeySet::new(loaded));
        Ok(Self {
            verifier: std::sync::Arc::new(verify::Verifier::new(keys, config.clone())),
            jwks,
            last_success: xmtp_common::time::Instant::now(),
        })
    }

    /// The public identity of every key loaded at startup, as `(kid, alg)`
    /// pairs. Published settings name the key set that was loaded; a later
    /// JWKS refresh does not change what was announced.
    pub fn published_keys(&self) -> Vec<(String, SigningAlgorithm)> {
        self.verifier
            .keys
            .0
            .load()
            .iter()
            .filter_map(|key| {
                Some((
                    key.kid.clone().unwrap_or_default(),
                    keys::signing_algorithm(key.alg)?,
                ))
            })
            .collect()
    }
}
