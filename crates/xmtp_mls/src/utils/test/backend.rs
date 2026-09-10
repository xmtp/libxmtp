//! A backend for native tests that need a specific configuration.
use xmtp_backend::{
    config::Config,
    test_support::{TestResult, TestServer},
};

mod tests;

/// Own the server and its disposable database. Drop stops the server and attempts
/// database cleanup, including when a test panics.
pub struct EphemeralBackend(TestServer);

impl EphemeralBackend {
    /// Validate partial TOML, create a database, migrate it, and start the server.
    pub async fn start(toml: &str) -> TestResult<Self> {
        Ok(Self(TestServer::from_toml(toml).await?))
    }

    pub fn url(&self) -> &str {
        &self.0.url
    }

    pub fn config(&self) -> &Config {
        &self.0.backend.config
    }

    /// Stop the server and remove its database. Return cleanup errors.
    pub async fn stop(self) -> TestResult {
        self.0.stop().await
    }
}
