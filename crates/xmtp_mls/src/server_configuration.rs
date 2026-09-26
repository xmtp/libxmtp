//! Resolving, storing, and refreshing what the backend publishes about itself.
//!
//! The client holds one immutable snapshot for its life
//! and reads configuration through the provider, never the database. This
//! module owns the three places the database is touched: the resolve that runs
//! once inside `build`, the refresh worker, and the explicit refresh the SDKs
//! expose.

use std::sync::Arc;

use parking_lot::RwLock;
use prost::Message;
use xmtp_api::{ApiClientWrapper, ApiError};
use xmtp_configuration::{
    BACKEND_DEFAULT_MAX_UPLOAD_BYTES, ConfigProvider, ServerConfiguration,
    ServerConfigurationError, StoredConfigProvider,
    attachments::{check_base_url, check_max_upload_bytes, check_retention_seconds},
};
use xmtp_db::prelude::*;
use xmtp_db::{StorageError, server_configuration::StoredServerConfiguration};
use xmtp_events::{ClientEvent, ClientRejectedByServer, EventWriter, RejectionCause};
use xmtp_proto::api_client::XmtpBackendClient;
use xmtp_proto::backend_v1;

use crate::client::ClientError;

/// Why a configuration read could not produce a usable copy.
///
/// Carried by [`ClientError::ConfigurationUnavailable`] so a caller can tell a
/// backend that refused from a database that would not accept the answer.
#[derive(Debug, thiserror::Error)]
pub enum ConfigurationFetchError {
    #[error("the deployment record could not be written: {0}")]
    StorageLocation(#[from] crate::storage_location::StorageLocationError),
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
            Self::StorageLocation(_) => false,
        }
    }
}

/// A condition that, once observed, fails later network work for the life of
/// the client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockedConnection {
    /// This database is bound to one deployment and another one answered.
    BackendMismatch { stored: String, received: String },
    /// The deployment now requires a newer client than this build.
    ClientVersionTooOld { client: String, minimum: String },
}

impl From<&BlockedConnection> for ClientError {
    fn from(blocked_connection: &BlockedConnection) -> Self {
        match blocked_connection {
            BlockedConnection::BackendMismatch { stored, received } => {
                ClientError::BackendMismatch {
                    stored: stored.clone(),
                    received: received.clone(),
                }
            }
            BlockedConnection::ClientVersionTooOld { client, minimum } => {
                ClientError::ClientVersionTooOld {
                    client: client.clone(),
                    minimum: minimum.clone(),
                }
            }
        }
    }
}

/// The snapshot the client was built with, plus a blocked connection a refresh may set.
///
/// Cloning shares both: every consumer sees the blocked connection when a
/// refresh sets it.
#[derive(Clone)]
pub struct ServerConfigurationHandle {
    deployment_recorder: Arc<RwLock<Option<crate::storage_location::DeploymentRecorder>>>,
    provider: Arc<dyn ConfigProvider>,
    blocked_connection: Arc<RwLock<Option<BlockedConnection>>>,
    event_writer: Arc<RwLock<Option<Arc<dyn EventWriter<()>>>>>,
    /// The chains an app-supplied smart contract wallet signature may name.
    /// `None` when the app supplied its own verifier.
    restricted_chains: Option<Arc<[String]>>,
}

impl std::fmt::Debug for ServerConfigurationHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerConfigurationHandle")
            .field("identifier", &self.configuration().identifier)
            .field("blocked_connection", &*self.blocked_connection.read())
            .finish()
    }
}

impl Default for ServerConfigurationHandle {
    fn default() -> Self {
        Self::new(Arc::new(StoredConfigProvider::default()))
    }
}

impl ServerConfigurationHandle {
    /// Hold one snapshot, with no zero left in its limits or attachment upload ceiling.
    ///
    /// Wire conversion replaces a zero on the wire with the compiled default, but a
    /// snapshot an app builds in Rust and hands in through a `ConfigProvider`
    /// never passes through that conversion, and a zero dimension
    /// would panic the `chunks(limit)` calls in `xmtp_api`. Every
    /// snapshot reaches a client through this constructor, so sanitizing here
    /// keeps zero limits out of all three readers at once: this handle, the
    /// wrapper that chunks with them, and the transport. It also removes an
    /// unusable attachment offer from a provider snapshot.
    pub fn new(provider: Arc<dyn ConfigProvider>) -> Self {
        let sanitized = {
            let supplied = provider.server_configuration();
            let limits = supplied.limits.without_zeroes();
            let attachments = supplied.attachments.as_ref().and_then(|offer| {
                let mut offer = offer.clone();
                if offer.max_upload_bytes == 0 {
                    offer.max_upload_bytes = BACKEND_DEFAULT_MAX_UPLOAD_BYTES;
                }
                let checked = check_base_url(&offer.base_url)
                    .and_then(|_| check_max_upload_bytes(offer.max_upload_bytes))
                    .and_then(|_| check_retention_seconds(offer.retention_seconds));
                if let Err(reason) = checked {
                    tracing::warn!(
                        field = reason.field(),
                        reason = reason.reason(),
                        "ignoring unusable attachment storage offer"
                    );
                    None
                } else {
                    Some(offer)
                }
            });
            (limits != supplied.limits || attachments != supplied.attachments).then(|| {
                ServerConfiguration {
                    limits,
                    attachments,
                    ..supplied.clone()
                }
            })
        };
        Self {
            deployment_recorder: Arc::default(),
            provider: match sanitized {
                Some(configuration) => Arc::new(StoredConfigProvider::new(configuration)),
                None => provider,
            },
            blocked_connection: Arc::default(),
            event_writer: Arc::default(),
            restricted_chains: None,
        }
    }

    /// Restrict app-supplied smart contract wallet signatures to the chains the
    /// snapshot names. Skipped entirely when the app
    /// supplied its own verifier, which the chain restriction exempts.
    pub(crate) fn with_chain_restriction(mut self, custom_verifier: bool) -> Self {
        self.restricted_chains = (!custom_verifier).then(|| {
            Arc::<[String]>::from(self.configuration().smart_contract_wallet_chains.clone())
        });
        self
    }

    /// Bind a signature request to the chains this deployment accepts, before
    /// it is handed to the app.
    pub fn restrict(&self, request: &mut xmtp_id::associations::builder::SignatureRequest) {
        if let Some(chains) = self.restricted_chains.clone() {
            request.restrict_chains(chains);
        }
    }

    /// The snapshot. Fixed for the life of the client: a refresh
    /// rewrites the stored row, never this value.
    pub fn configuration(&self) -> &ServerConfiguration {
        self.provider.server_configuration()
    }

    /// Whether this deployment keeps a commit log. Read at every
    /// write and read site, so a deployment that turns it off turns it off for
    /// every group this client touches.
    pub fn commit_log_enabled(&self) -> bool {
        self.configuration().mls.commit_log_enabled()
    }

    /// The blocked connection, if one has been observed.
    pub fn blocked_connection(&self) -> Option<BlockedConnection> {
        self.blocked_connection.read().clone()
    }

    pub(crate) fn set_event_writer(&self, writer: Arc<dyn EventWriter<()>>) {
        *self.event_writer.write() = Some(writer);
    }

    pub(crate) fn set_deployment_recorder(
        &self,
        recorder: crate::storage_location::DeploymentRecorder,
    ) {
        *self.deployment_recorder.write() = Some(recorder);
    }

    async fn record_deployment(&self, identifier: &str) -> Result<(), ClientError> {
        let recorder = self.deployment_recorder.read().clone();
        if let Some(recorder) = recorder {
            recorder.record(identifier).await.map_err(|error| {
                ClientError::ConfigurationUnavailable(Box::new(
                    ConfigurationFetchError::StorageLocation(error),
                ))
            })?;
        }
        Ok(())
    }

    /// Fail when the connection is blocked. Every call that reaches the network goes
    /// through here.
    // implements: CONF-075
    pub fn check(&self) -> Result<(), ClientError> {
        match self.blocked_connection.read().as_ref() {
            Some(blocked_connection) => Err(blocked_connection.into()),
            None => Ok(()),
        }
    }

    /// Block the connection on the first failure. A later one does not displace it: the
    /// first cause is the one worth reporting.
    pub(crate) fn block_connection(&self, reason: BlockedConnection) -> ClientError {
        let mut guard = self.blocked_connection.write();
        if let Some(held) = guard.as_ref() {
            return held.into();
        }
        let error = ClientError::from(&reason);
        let event = match &reason {
            BlockedConnection::BackendMismatch { .. } => ClientRejectedByServer {
                cause: RejectionCause::BackendMismatch,
                min_libxmtp_version: None,
            },
            BlockedConnection::ClientVersionTooOld { minimum, .. } => ClientRejectedByServer {
                cause: RejectionCause::VersionTooOld,
                min_libxmtp_version: Some(minimum.clone()),
            },
        };
        *guard = Some(reason);
        drop(guard);
        if let Some(writer) = self.event_writer.read().as_ref() {
            writer.emit(Some(ClientEvent::ClientRejectedByServer(event)), None);
        }
        error
    }
}

/// Decode a stored row into a snapshot.
///
/// A copy that does not decode is a warning, not a failure. The
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

/// Validate a fetched response and turn it into a snapshot.
pub(crate) fn validated(
    response: &backend_v1::GetConfigurationResponse,
) -> Result<ServerConfiguration, ServerConfigurationError> {
    let configuration = ServerConfiguration::from(response.clone());
    configuration.validate()?;
    Ok(configuration)
}

/// Fetch, validate, and store one copy, applying the identifier binding.
///
/// Shared by the build-time fetch, the refresh worker,
/// and the explicit refresh the SDKs expose.
// implements: CONF-030, CONF-040, CONF-071
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

    // The identifier, not the URL, is the binding. A row with an
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
        // A failure to record it keeps the connection blocked in memory, so this
        // client still fails every later call.
        if let Err(error) = db.record_server_configuration_conflict(&configuration.identifier) {
            tracing::error!(%error, "could not record the conflicting backend identifier");
        }
        return Err(handle.block_connection(BlockedConnection::BackendMismatch {
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

    handle.record_deployment(&configuration.identifier).await?;

    Ok(configuration)
}

/// Read a deployment's configuration with no database and no client.
///
/// The one call that needs no credential, so an app can learn whether
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

/// The form a backend URL is stored and compared in.
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

/// Resolve the snapshot `build` will hold.
///
/// The minimum-version check is not here: it applies to
/// the snapshot the client ends up holding, which may have come from a
/// caller-supplied provider that never reached this function.
///
/// Runs before any identity work. Offline, it never awaits a network call, so
/// `build_offline` still completes without polling a pending future.
// implements: CONF-026, CONF-027, CONF-033, CONF-034, CONF-072
pub(crate) async fn resolve<ApiClient>(
    api: &ApiClientWrapper<ApiClient>,
    db: &impl DbQuery,
    allow_offline: bool,
) -> Result<ServerConfigurationHandle, ClientError>
where
    ApiClient: XmtpBackendClient,
{
    let stored = db.server_configuration().map_err(storage_unavailable)?;

    // A recorded conflict fails every build until the database is
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
        // Offline with no copy is compiled defaults and an empty
        // identifier. The first successful refresh writes the row.
        (true, None) => ServerConfiguration::default(),
        // Offline with a copy uses it, and skips the URL check.
        (true, Some(stored)) => snapshot_from(&stored),
        // Online with no copy fetches before any identity work.
        (false, None) => {
            let handle = ServerConfigurationHandle::default();
            fetch_and_store(api, db, &handle).await?
        }
        (false, Some(stored)) => {
            // The stored copy is used as is unless the
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

/// Compare on major, minor, and patch only, so a
/// prerelease tag never makes a client too old.
// implements: CONF-049
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
