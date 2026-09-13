//! JWT verification and bounded signing-key refresh.
pub(crate) mod jwks;
pub(crate) mod keys;
pub(crate) mod verify;

pub use verify::AuthContext;

pub(crate) struct Authentication {
    pub verifier: std::sync::Arc<verify::Verifier>,
    pub jwks: Option<jwks::JwksSource>,
    pub last_success: xmtp_common::time::Instant,
}

impl Authentication {
    /// Load keys before any listener binds. Startup fetch errors contain only the host.
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
}
