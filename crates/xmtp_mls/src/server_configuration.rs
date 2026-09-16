//! Resolving, storing, and refreshing what the backend publishes about itself.
//!
//! Spec 006 §6. The client holds one immutable snapshot for its life (CFG-030)
//! and reads every value in §6.4 through the provider, never the database. This
//! module owns the three places the database is touched: the resolve that runs
//! once inside `build`, the refresh worker, and the explicit refresh the SDKs
//! expose.

use std::sync::Arc;

use parking_lot::RwLock;
use prost::Message;
use xmtp_api::{ApiClientWrapper, ApiError};
use xmtp_configuration::{
    ConfigProvider, ServerConfiguration, ServerConfigurationError, StoredConfigProvider,
};
use xmtp_db::prelude::*;
use xmtp_db::{StorageError, server_configuration::StoredServerConfiguration};
use xmtp_proto::api_client::XmtpBackendClient;
use xmtp_proto::backend_v1;

use crate::client::ClientError;

/// Why a configuration read could not produce a usable copy.
///
/// Carried by [`ClientError::ConfigurationUnavailable`] so a caller can tell a
/// backend that refused from a database that would not accept the answer.
#[derive(Debug, thiserror::Error)]
pub enum ConfigurationFetchError {
    /// The backend did not answer, or answered with an error. A backend with
    /// no `ConfigurationService` answers `UNIMPLEMENTED`; there is no shim.
    #[error("the backend did not serve its configuration: {0}")]
    Api(#[from] ApiError),
    /// The answer arrived but could not be stored.
    #[error("the fetched configuration could not be stored: {0}")]
    Storage(#[from] StorageError),
}

impl xmtp_common::RetryableError for ConfigurationFetchError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Api(e) => e.is_retryable(),
            Self::Storage(e) => e.is_retryable(),
        }
    }
}

/// A condition that, once observed, fails every later call for the life of the
/// client (CFG-051, CFG-054, CFG-061).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigurationLatch {
    /// This database is bound to one deployment and another one answered.
    BackendMismatch { stored: String, received: String },
    /// The deployment now requires a newer client than this build.
    ClientVersionTooOld { client: String, minimum: String },
}

impl From<&ConfigurationLatch> for ClientError {
    fn from(latch: &ConfigurationLatch) -> Self {
        match latch {
            ConfigurationLatch::BackendMismatch { stored, received } => {
                ClientError::BackendMismatch {
                    stored: stored.clone(),
                    received: received.clone(),
                }
            }
            ConfigurationLatch::ClientVersionTooOld { client, minimum } => {
                ClientError::ClientVersionTooOld {
                    client: client.clone(),
                    minimum: minimum.clone(),
                }
            }
        }
    }
}

/// The snapshot the client was built with, plus the latch a refresh may set.
///
/// Cloning shares both: every consumer sees the same latch the moment a
/// refresh trips it.
#[derive(Clone)]
pub struct ServerConfigurationHandle {
    provider: Arc<dyn ConfigProvider>,
    latch: Arc<RwLock<Option<ConfigurationLatch>>>,
    /// The chains an app-supplied smart contract wallet signature may name
    /// (CFG-069, CFG-070). `None` when the app supplied its own verifier,
    /// which CFG-069 exempts from the check.
    restricted_chains: Option<Arc<[String]>>,
}

impl std::fmt::Debug for ServerConfigurationHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerConfigurationHandle")
            .field("identifier", &self.configuration().identifier)
            .field("latch", &*self.latch.read())
            .finish()
    }
}

impl Default for ServerConfigurationHandle {
    fn default() -> Self {
        Self::new(Arc::new(StoredConfigProvider::default()))
    }
}

impl ServerConfigurationHandle {
    pub fn new(provider: Arc<dyn ConfigProvider>) -> Self {
        Self {
            provider,
            latch: Arc::default(),
            restricted_chains: None,
        }
    }

    /// Restrict app-supplied smart contract wallet signatures to the chains the
    /// snapshot names (CFG-069, CFG-070). Skipped entirely when the app
    /// supplied its own verifier, which CFG-069 exempts.
    pub(crate) fn with_chain_restriction(mut self, custom_verifier: bool) -> Self {
        self.restricted_chains = (!custom_verifier).then(|| {
            Arc::<[String]>::from(self.configuration().smart_contract_wallet_chains.clone())
        });
        self
    }

    /// Bind a signature request to the chains this deployment accepts, before
    /// it is handed to the app (CFG-069, CFG-070).
    pub fn restrict(&self, request: &mut xmtp_id::associations::builder::SignatureRequest) {
        if let Some(chains) = self.restricted_chains.clone() {
            request.restrict_chains(chains);
        }
    }

    /// The snapshot. Fixed for the life of the client (CFG-030): a refresh
    /// rewrites the stored row, never this value.
    pub fn configuration(&self) -> &ServerConfiguration {
        self.provider.server_configuration()
    }

    /// Whether this deployment keeps a commit log (CFG-068). Read at every
    /// write and read site, so a deployment that turns it off turns it off for
    /// every group this client touches.
    pub fn commit_log_enabled(&self) -> bool {
        self.configuration().mls.commit_log_enabled()
    }

    /// The latched failure, if one has been observed.
    pub fn latched(&self) -> Option<ConfigurationLatch> {
        self.latch.read().clone()
    }

    /// Fail when a latch is set. Every call that reaches the network goes
    /// through here (CFG-051, CFG-061).
    pub fn check(&self) -> Result<(), ClientError> {
        match self.latch.read().as_ref() {
            Some(latch) => Err(latch.into()),
            None => Ok(()),
        }
    }

    /// Latch the first failure seen. A later one does not displace it: the
    /// first cause is the one worth reporting.
    pub(crate) fn latch(&self, latch: ConfigurationLatch) -> ClientError {
        let mut guard = self.latch.write();
        let held = guard.get_or_insert(latch);
        ClientError::from(&*held)
    }
}

/// Decode a stored row into a snapshot.
///
/// CFG-042: a copy that does not decode is a warning, not a failure. The
/// identifier stays available for the binding check and every value falls back
/// to its compiled default.
fn snapshot_from(stored: &StoredServerConfiguration) -> ServerConfiguration {
    match backend_v1::GetConfigurationResponse::decode(stored.response.as_slice()) {
        Ok(response) => ServerConfiguration::from(response),
        Err(error) => {
            tracing::warn!(
                identifier = %stored.identifier,
                %error,
                "stored server configuration does not decode; using compiled defaults"
            );
            ServerConfiguration {
                identifier: stored.identifier.clone(),
                ..Default::default()
            }
        }
    }
}

/// Validate a fetched response and turn it into a snapshot (CFG-044).
fn validated(
    response: &backend_v1::GetConfigurationResponse,
) -> Result<ServerConfiguration, ServerConfigurationError> {
    let configuration = ServerConfiguration::from(response.clone());
    configuration.validate()?;
    Ok(configuration)
}

/// Fetch, validate, and store one copy, applying the identifier binding.
///
/// Shared by the build-time fetch (CFG-040, CFG-055), the refresh worker
/// (CFG-048), and the explicit refresh the SDKs expose (CFG-082).
pub(crate) async fn fetch_and_store<ApiClient>(
    api: &ApiClientWrapper<ApiClient>,
    db: &impl DbQuery,
    handle: &ServerConfigurationHandle,
) -> Result<ServerConfiguration, ClientError>
where
    ApiClient: XmtpBackendClient,
{
    handle.check()?;

    let response = api.get_configuration().await.map_err(|error| {
        ClientError::ConfigurationUnavailable(Box::new(ConfigurationFetchError::Api(error)))
    })?;
    let configuration = validated(&response)?;

    // CFG-051: the identifier, not the URL, is the binding. A row with an
    // empty identifier was written by an offline build and binds nothing.
    let stored = db.server_configuration().map_err(storage_unavailable)?;
    if let Some(stored) = stored.as_ref()
        && !stored.identifier.is_empty()
        && stored.identifier != configuration.identifier
    {
        tracing::error!(
            stored = %stored.identifier,
            received = %configuration.identifier,
            "this database is bound to a different backend deployment"
        );
        // CFG-054: a failure to record it keeps the in-memory latch, so this
        // client still fails every later call.
        if let Err(error) = db.record_server_configuration_conflict(&configuration.identifier) {
            tracing::error!(%error, "could not record the conflicting backend identifier");
        }
        return Err(handle.latch(ConfigurationLatch::BackendMismatch {
            stored: stored.identifier.clone(),
            received: configuration.identifier.clone(),
        }));
    }

    db.store_server_configuration(
        &configuration.identifier,
        normalized_url(api.backend_url().unwrap_or_default()),
        &response.encode_to_vec(),
        xmtp_common::time::now_ns(),
    )
    .map_err(storage_unavailable)?;

    Ok(configuration)
}

/// Read a deployment's configuration with no database and no client (CFG-081).
///
/// The one call that needs no credential (CFG-045), so an app can learn whether
/// the deployment requires authentication, which scopes it wants, and which
/// chains it accepts before it decides how to build a client. Nothing is
/// stored and no identifier binding is applied: there is no database to bind to.
pub async fn fetch_server_configuration<ApiClient>(
    api: &ApiClientWrapper<ApiClient>,
) -> Result<ServerConfiguration, ClientError>
where
    ApiClient: XmtpBackendClient,
{
    let response = api.get_configuration().await.map_err(|error| {
        ClientError::ConfigurationUnavailable(Box::new(ConfigurationFetchError::Api(error)))
    })?;
    Ok(validated(&response)?)
}

/// The form a backend URL is stored and compared in (CFG-055).
///
/// A transport reports the URI it dialled, which for `http://host:port` carries
/// a trailing slash the app never typed. Normalising both sides keeps a purely
/// cosmetic difference from looking like a move to another deployment.
fn normalized_url(url: &str) -> &str {
    url.trim_end_matches('/')
}

fn storage_unavailable(error: StorageError) -> ClientError {
    ClientError::ConfigurationUnavailable(Box::new(ConfigurationFetchError::Storage(error)))
}

/// Resolve the snapshot `build` will hold (CFG-040 to CFG-043, CFG-052,
/// CFG-055).
///
/// The minimum-version check of CFG-060 is deliberately not here: it applies to
/// the snapshot the client ends up holding, which may have come from a
/// caller-supplied provider (CFG-033) that never reached this function.
///
/// Runs before any identity work. Offline, it never awaits a network call, so
/// `build_offline` still completes without polling a pending future (CFG-094).
pub(crate) async fn resolve<ApiClient>(
    api: &ApiClientWrapper<ApiClient>,
    db: &impl DbQuery,
    allow_offline: bool,
) -> Result<ServerConfigurationHandle, ClientError>
where
    ApiClient: XmtpBackendClient,
{
    let stored = db.server_configuration().map_err(storage_unavailable)?;

    // CFG-052: a recorded conflict fails every build until the database is
    // replaced with one created for the backend the app now uses.
    if let Some(conflicting) = stored
        .as_ref()
        .and_then(|row| row.conflicting_identifier.clone())
    {
        return Err(ClientError::BackendMismatch {
            stored: stored.map(|row| row.identifier).unwrap_or_default(),
            received: conflicting,
        });
    }

    let configuration = match (allow_offline, stored) {
        // CFG-043: offline with no copy is compiled defaults and an empty
        // identifier. The first successful refresh writes the row.
        (true, None) => ServerConfiguration::default(),
        // CFG-043: offline with a copy uses it, and skips the URL check.
        (true, Some(stored)) => snapshot_from(&stored),
        // CFG-040: online with no copy fetches before any identity work.
        (false, None) => {
            let handle = ServerConfigurationHandle::default();
            fetch_and_store(api, db, &handle).await?
        }
        (false, Some(stored)) => {
            // CFG-042 and CFG-055: the stored copy is used as is unless the
            // configured URL moved, in which case the identifier is checked
            // again before anything else happens.
            let moved = api
                .backend_url()
                .is_some_and(|url| normalized_url(url) != normalized_url(&stored.backend_url));
            if moved {
                tracing::info!(
                    from = %stored.backend_url,
                    to = api.backend_url().unwrap_or_default(),
                    "backend URL changed; re-reading the deployment configuration"
                );
                let handle = ServerConfigurationHandle::default();
                fetch_and_store(api, db, &handle).await?
            } else {
                snapshot_from(&stored)
            }
        }
    };

    Ok(ServerConfigurationHandle::new(Arc::new(
        StoredConfigProvider::new(configuration),
    )))
}

/// CFG-060 and CFG-061: compare on major, minor, and patch only, so a
/// prerelease tag never makes a client too old.
pub(crate) fn check_minimum_version(
    configuration: &ServerConfiguration,
    client_version: &semver::Version,
) -> Result<(), ClientError> {
    let Some(minimum) = configuration.minimum_version()? else {
        return Ok(());
    };
    if xmtp_configuration::version_is_below(client_version, &minimum) {
        return Err(ClientError::ClientVersionTooOld {
            client: client_version.to_string(),
            minimum: minimum.to_string(),
        });
    }
    Ok(())
}

pub mod worker;

#[cfg(test)]
mod tests;
