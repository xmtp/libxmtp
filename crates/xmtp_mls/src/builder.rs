use crate::{
    GroupCommitLock, StorageError, XmtpApi,
    client::{Client, DeviceSync},
    context::{XmtpMlsLocalContext, XmtpSharedContext},
    groups::{
        device_sync::worker::SyncWorker, disappearing_messages::DisappearingMessagesWorker,
        key_package_cleaner_worker::KeyPackagesCleanerWorker,
        pending_self_remove_worker::PendingSelfRemoveWorker,
    },
    identity::{Identity, IdentityStrategy},
    identity_updates::load_identity_updates,
    mutex_registry::MutexRegistry,
    utils::{VersionInfo, cleanup_duplicate_updates},
    worker::WorkerRunner,
};
use futures::FutureExt;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::broadcast;
use tracing::debug;
use xmtp_api::{ApiClientWrapper, ApiDebugWrapper};
use xmtp_api_d14n::{
    TrackedStatsClient,
    protocol::{CursorStore, XmtpQuery},
};
use xmtp_common::{Event, Retry};
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_db::{DbConnection, XmtpMlsStorageProvider};
use xmtp_db::{XmtpDb, sql_key_store::SqlKeyStore};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_macro::log_event;

type ContextParts<Api, S, Db> = Arc<XmtpMlsLocalContext<Api, Db, S>>;

#[derive(Error, Debug, xmtp_common::ErrorCode)]
pub enum ClientBuilderError {
    #[error(transparent)]
    #[error_code(inherit)]
    AddressValidation(#[from] IdentifierValidationError),
    #[error("Missing parameter: {parameter}")]
    MissingParameter { parameter: &'static str },
    #[error(transparent)]
    ClientError(#[from] crate::client::ClientError),
    #[error("Storage Error")]
    StorageError(#[from] StorageError),
    #[error(transparent)]
    Identity(#[from] crate::identity::IdentityError),
    #[error(transparent)]
    WrappedApiError(#[from] xmtp_api::ApiError),
    #[error(transparent)]
    GroupError(#[from] Box<crate::groups::GroupError>),
    #[error(transparent)]
    DeviceSync(#[from] Box<crate::groups::device_sync::DeviceSyncError>),
    #[error("Offline build failed, builder tried to access the network")]
    OfflineBuildFailed,
}

impl From<crate::groups::device_sync::DeviceSyncError> for ClientBuilderError {
    fn from(value: crate::groups::device_sync::DeviceSyncError) -> Self {
        ClientBuilderError::DeviceSync(Box::new(value))
    }
}

impl From<crate::groups::GroupError> for ClientBuilderError {
    fn from(value: crate::groups::GroupError) -> Self {
        ClientBuilderError::GroupError(Box::new(value))
    }
}

pub struct ClientBuilder<ApiClient, S, Db = xmtp_db::DefaultStore> {
    pub(crate) api_client: Option<ApiClient>,
    pub(crate) identity: Option<Identity>,
    pub(crate) store: Option<Db>,
    pub(crate) identity_strategy: IdentityStrategy,
    pub(crate) scw_verifier: Option<Box<dyn SmartContractSignatureVerifier>>,
    pub(crate) device_sync_server_url: Option<String>,
    pub(crate) device_sync_worker_mode: SyncWorkerMode,
    pub(crate) fork_recovery_opts: Option<ForkRecoveryOpts>,
    pub(crate) version_info: VersionInfo,
    pub(crate) allow_offline: bool,
    pub(crate) disable_commit_log_worker: bool,
    pub(crate) mls_storage: Option<S>,
    pub(crate) sync_api_client: Option<ApiClient>,
    pub(crate) cursor_store: Option<Arc<dyn CursorStore>>,
    pub(crate) disable_workers: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncWorkerMode {
    Disabled,
    Enabled,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ForkRecoveryPolicy {
    None,
    AllowlistedGroups,
    All,
}

#[derive(Clone, Debug)]
pub struct ForkRecoveryOpts {
    pub enable_recovery_requests: ForkRecoveryPolicy,
    pub groups_to_request_recovery: Vec<String>,
    pub disable_recovery_responses: bool,
    pub worker_interval_ns: Option<u64>,
}

impl Default for ForkRecoveryOpts {
    fn default() -> Self {
        Self {
            enable_recovery_requests: ForkRecoveryPolicy::None,
            groups_to_request_recovery: Vec::new(),
            disable_recovery_responses: false,
            worker_interval_ns: None,
        }
    }
}

impl Client<()> {
    /// Get the builder for this [`Client`]
    pub fn builder(strategy: IdentityStrategy) -> ClientBuilder<(), ()> {
        ClientBuilder::<(), ()>::new(strategy)
    }
}

impl<ApiClient, S, Db> ClientBuilder<ApiClient, S, Db> {
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn new(identity_strategy: IdentityStrategy) -> Self {
        Self {
            identity_strategy,
            api_client: None,
            identity: None,
            store: None,
            scw_verifier: None,
            device_sync_server_url: None,
            device_sync_worker_mode: SyncWorkerMode::Enabled,
            fork_recovery_opts: None,
            version_info: VersionInfo::default(),
            allow_offline: false,
            disable_commit_log_worker: false,
            mls_storage: None,
            sync_api_client: None,
            cursor_store: None,
            disable_workers: false,
        }
    }
}

#[cfg(test)]
impl<ApiClient, S, Db> ClientBuilder<ApiClient, S, Db>
where
    ApiClient: Clone,
    Db: Clone,
    S: Clone,
{
    pub fn from_client(
        client: Client<ContextParts<ApiClient, S, Db>>,
    ) -> ClientBuilder<ApiClient, S, Db> {
        let cloned_api: ApiClient = client.context.api_client.clone().api_client;
        let cloned_sync_api: ApiClient = client.context.sync_api_client.clone().api_client;
        ClientBuilder {
            api_client: Some(cloned_api),
            identity: Some(client.context.identity.clone()),
            store: Some(client.context.store.clone()),
            identity_strategy: IdentityStrategy::CachedOnly,
            scw_verifier: Some(Box::new(client.context.scw_verifier.clone())),
            device_sync_server_url: client.context.device_sync.server_url.clone(),
            device_sync_worker_mode: client.context.device_sync.mode,
            fork_recovery_opts: Some(client.context.fork_recovery_opts.clone()),
            version_info: client.context.version_info.clone(),
            allow_offline: false,
            disable_commit_log_worker: false,
            mls_storage: Some(client.context.mls_storage.clone()),
            sync_api_client: Some(cloned_sync_api),
            cursor_store: None,
            disable_workers: false,
        }
    }
}

// TODO: the return type is temp and
// will be modified in subsequent PRs
impl<ApiClient, S, Db> ClientBuilder<ApiClient, S, Db> {
    /// build a client in offline mode.
    /// returns an error if the client failed to build as offline
    pub fn build_offline(self) -> Result<Client<ContextParts<ApiClient, S, Db>>, ClientBuilderError>
    where
        ApiClient: XmtpApi + XmtpQuery + 'static,
        Db: xmtp_db::XmtpDb + 'static,
        S: XmtpMlsStorageProvider + 'static,
    {
        self.build()
            .now_or_never()
            .ok_or(ClientBuilderError::OfflineBuildFailed)
            .flatten()
    }

    pub async fn build(self) -> Result<Client<ContextParts<ApiClient, S, Db>>, ClientBuilderError>
    where
        ApiClient: XmtpApi + XmtpQuery + 'static,
        Db: xmtp_db::XmtpDb + 'static,
        S: XmtpMlsStorageProvider + 'static,
    {
        let ClientBuilder {
            mut api_client,
            identity,
            mut store,
            identity_strategy,
            mut scw_verifier,

            device_sync_server_url,
            device_sync_worker_mode,
            fork_recovery_opts,
            version_info,
            allow_offline,
            disable_commit_log_worker,
            mut mls_storage,
            mut sync_api_client,
            // cursor_store,
            disable_workers,
            ..
        } = self;

        let api_client = api_client
            .take()
            .ok_or(ClientBuilderError::MissingParameter {
                parameter: "api_client",
            })?;

        let sync_api_client =
            sync_api_client
                .take()
                .ok_or(ClientBuilderError::MissingParameter {
                    parameter: "sync_api_client",
                })?;

        let scw_verifier = scw_verifier
            .take()
            .ok_or(ClientBuilderError::MissingParameter {
                parameter: "scw_verifier",
            })?;

        let store = store
            .take()
            .ok_or(ClientBuilderError::MissingParameter { parameter: "store" })?;

        let mls_storage = mls_storage
            .take()
            .ok_or(ClientBuilderError::MissingParameter {
                parameter: "mls_storage",
            })?;

        let api_client = ApiClientWrapper::new(api_client, Retry::default());
        let sync_api_client = ApiClientWrapper::new(sync_api_client, Retry::default());
        let conn = store.db();
        let identity = if let Some(identity) = identity {
            identity
        } else {
            identity_strategy
                .initialize_identity(&api_client, &mls_storage, &scw_verifier)
                .await?
        };

        debug!(
            inbox_id = identity.inbox_id(),
            installation_id = hex::encode(identity.installation_keys.public_bytes()),
            "Initialized identity"
        );
        if !allow_offline {
            // get sequence_id from identity updates and loaded into the DB
            load_identity_updates(
                &api_client,
                &conn,
                vec![identity.inbox_id.as_str()].as_slice(),
            )
            .await?;
        }

        let (local_events, _) = broadcast::channel(32);
        let (worker_tx, _) = broadcast::channel(32);
        let mut workers = WorkerRunner::new();
        let context = Arc::new(XmtpMlsLocalContext {
            identity,
            mls_storage,
            store,
            api_client,
            version_info,
            scw_verifier: Arc::new(scw_verifier),
            mutexes: MutexRegistry::new(),
            mls_commit_lock: Arc::new(GroupCommitLock::new()),
            local_events: local_events.clone(),
            worker_events: worker_tx.clone(),
            device_sync: DeviceSync {
                server_url: device_sync_server_url,
                mode: device_sync_worker_mode,
            },
            fork_recovery_opts: fork_recovery_opts.unwrap_or_default(),

            sync_api_client,
            worker_metrics: workers.metrics().clone(),
            task_channels: workers.task_channels().clone(),
        });

        // register workers
        if !disable_workers {
            if context.device_sync_worker_enabled() {
                workers.register_new_worker::<SyncWorker<ContextParts<ApiClient, S, Db>>, _>(
                    context.clone(),
                );
            }
            workers
                .register_new_worker::<KeyPackagesCleanerWorker<ContextParts<ApiClient, S, Db>>, _>(
                    context.clone(),
                );
            workers
                .register_new_worker::<DisappearingMessagesWorker<ContextParts<ApiClient, S, Db>>, _>(
                    context.clone(),
                );
            workers
                .register_new_worker::<PendingSelfRemoveWorker<ContextParts<ApiClient, S, Db>>, _>(
                    context.clone(),
                );
            // Enable CommitLogWorker based on configuration
            if xmtp_configuration::ENABLE_COMMIT_LOG && !disable_commit_log_worker {
                workers.register_new_worker::<
                crate::groups::commit_log::CommitLogWorker<ContextParts<ApiClient, S, Db>>,
                _,
                >(context.clone());
            }
            workers
                .register_new_worker::<crate::tasks::TaskWorker<ContextParts<ApiClient, S, Db>>, _>(
                    context.clone(),
                );
        }

        let workers = Arc::new(workers);

        if !disable_workers {
            workers.spawn(context.clone());
        }

        log_event!(
            Event::ClientCreated,
            context.installation_id(),
            inbox_id = context.inbox_id(),
            full_installation_id = hex::encode(context.installation_id()),
            device_sync_enabled = context.device_sync_worker_enabled(),
            disabled_workers = disable_workers
        );

        let client = Client {
            context,
            local_events,
            workers,
        };

        // Cleanup old unstitched group updated messages.
        let conn = DbConnection::new(client.db());
        xmtp_common::spawn(None, cleanup_duplicate_updates::perform(conn));

        Ok(client)
    }

    pub fn identity(self, identity: Identity) -> Self {
        Self {
            identity: Some(identity),
            ..self
        }
    }

    pub fn store<NewDb>(self, db: NewDb) -> ClientBuilder<ApiClient, S, NewDb> {
        ClientBuilder {
            store: Some(db),
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self.mls_storage,
            sync_api_client: self.sync_api_client,
            cursor_store: self.cursor_store,
            disable_workers: self.disable_workers,
        }
    }

    /// Use the default SQlite MLS Key-Value Store
    pub fn default_mls_store(
        self,
    ) -> Result<
        ClientBuilder<ApiClient, SqlKeyStore<<Db as XmtpDb>::DbQuery>, Db>,
        ClientBuilderError,
    >
    where
        Db: XmtpDb,
    {
        Ok(ClientBuilder {
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: Some(SqlKeyStore::new(
                self.store
                    .as_ref()
                    .ok_or(ClientBuilderError::MissingParameter {
                        parameter: "encrypted store",
                    })?
                    .db(),
            )),
            store: self.store,
            sync_api_client: self.sync_api_client,
            cursor_store: self.cursor_store,
            disable_workers: self.disable_workers,
        })
    }

    pub fn mls_storage<NewS>(self, mls_storage: NewS) -> ClientBuilder<ApiClient, NewS, Db> {
        ClientBuilder {
            store: self.store,
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: Some(mls_storage),
            sync_api_client: self.sync_api_client,
            cursor_store: self.cursor_store,
            disable_workers: self.disable_workers,
        }
    }

    pub fn with_device_sync_server_url(self, url: Option<String>) -> Self {
        Self {
            device_sync_server_url: url,
            ..self
        }
    }

    pub fn with_disable_workers(mut self, disable_workers: bool) -> Self {
        self.disable_workers = disable_workers;
        self
    }

    pub fn device_sync_server_url(self, url: &str) -> Self {
        Self {
            device_sync_server_url: Some(url.into()),
            ..self
        }
    }

    pub fn with_device_sync_worker_mode(self, mode: Option<SyncWorkerMode>) -> Self {
        Self {
            device_sync_worker_mode: mode.unwrap_or(SyncWorkerMode::Enabled),
            ..self
        }
    }

    pub fn device_sync_worker_mode(self, mode: SyncWorkerMode) -> Self {
        Self {
            device_sync_worker_mode: mode,
            ..self
        }
    }

    pub fn fork_recovery_opts(self, opts: ForkRecoveryOpts) -> Self {
        Self {
            fork_recovery_opts: Some(opts),
            ..self
        }
    }

    pub fn api_clients<A>(self, api_client: A, sync_api_client: A) -> ClientBuilder<A, S, Db> {
        // let api_retry = Retry::builder().build();
        // let api_client = ApiClientWrapper::new(api_client, api_retry.clone());
        // let sync_api_client = ApiClientWrapper::new(sync_api_client, api_retry.clone());
        ClientBuilder {
            api_client: Some(api_client),
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            store: self.store,
            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self.mls_storage,
            sync_api_client: Some(sync_api_client),
            cursor_store: self.cursor_store,
            disable_workers: self.disable_workers,
        }
    }

    pub fn cursor_store(
        self,
        cursor_store: Arc<dyn CursorStore>,
    ) -> ClientBuilder<ApiClient, S, Db> {
        Self {
            cursor_store: Some(cursor_store),
            ..self
        }
    }

    pub fn maybe_version(
        mut self,
        version: Option<VersionInfo>,
    ) -> ClientBuilder<ApiClient, S, Db> {
        if let Some(v) = version {
            self.version_info = v;
        }
        self
    }

    pub fn version(self, version_info: VersionInfo) -> ClientBuilder<ApiClient, S, Db> {
        Self {
            version_info,
            ..self
        }
    }

    /// Skip network calls when building a client
    pub fn with_allow_offline(
        self,
        allow_offline: Option<bool>,
    ) -> ClientBuilder<ApiClient, S, Db> {
        Self {
            allow_offline: allow_offline.unwrap_or(false),
            ..self
        }
    }

    /// Control whether the CommitLogWorker background task is enabled.
    /// Useful for tests that need deterministic commit log operations.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn with_commit_log_worker(self, enabled: bool) -> Self {
        Self {
            disable_commit_log_worker: !enabled,
            ..self
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn enable_sqlite_triggers(self) -> Self
    where
        Db: XmtpDb,
    {
        let db = self.store.as_ref().expect("unwrapping in test env").conn();
        let db = xmtp_db::DbConnection::new(db);
        db.register_triggers();
        db.disable_memory_security();
        self
    }

    /// Wrap the Api Client in a Debug Adapter which prints api stats on error.
    /// Requires the api client to be set in the builder.
    pub fn enable_api_debug_wrapper(
        self,
    ) -> Result<ClientBuilder<ApiDebugWrapper<ApiClient>, S, Db>, ClientBuilderError> {
        if self.api_client.is_none() || self.sync_api_client.is_none() {
            return Err(ClientBuilderError::MissingParameter {
                parameter: "api_client",
            });
        }

        Ok(ClientBuilder {
            api_client: Some(ApiDebugWrapper::new(
                self.api_client.expect("checked for none"),
            )),
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            store: self.store,

            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self.mls_storage,
            sync_api_client: Some(ApiDebugWrapper::new(
                self.sync_api_client.expect("checked for none"),
            )),
            cursor_store: self.cursor_store,
            disable_workers: self.disable_workers,
        })
    }

    pub fn enable_api_stats(
        self,
    ) -> Result<ClientBuilder<TrackedStatsClient<ApiClient>, S, Db>, ClientBuilderError> {
        if self.api_client.is_none() || self.sync_api_client.is_none() {
            return Err(ClientBuilderError::MissingParameter {
                parameter: "api_client",
            });
        }

        Ok(ClientBuilder {
            api_client: Some(TrackedStatsClient::new(
                self.api_client.expect("checked for none"),
            )),
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            store: self.store,

            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self.mls_storage,
            sync_api_client: Some(TrackedStatsClient::new(
                self.sync_api_client.expect("checked for none"),
            )),
            cursor_store: self.cursor_store,
            disable_workers: self.disable_workers,
        })
    }

    pub fn with_scw_verifier(
        self,
        verifier: impl SmartContractSignatureVerifier + 'static,
    ) -> ClientBuilder<ApiClient, S, Db> {
        ClientBuilder {
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: Some(Box::new(verifier)),
            store: self.store,

            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self.mls_storage,
            sync_api_client: self.sync_api_client,
            cursor_store: self.cursor_store,
            disable_workers: self.disable_workers,
        }
    }

    /// Build the client with a default remote verifier
    /// requires the 'api' to be set.
    pub fn with_remote_verifier(self) -> Result<ClientBuilder<ApiClient, S, Db>, ClientBuilderError>
    where
        ApiClient: Clone + XmtpApi + 'static,
    {
        let api = self
            .api_client
            .clone()
            .ok_or(ClientBuilderError::MissingParameter {
                parameter: "api_client",
            })?;

        Ok(ClientBuilder {
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: Some(Box::new(ApiClientWrapper::new(api, Retry::default()))
                as Box<dyn SmartContractSignatureVerifier>),
            store: self.store,
            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self.mls_storage,
            sync_api_client: self.sync_api_client,
            cursor_store: self.cursor_store,
            disable_workers: self.disable_workers,
        })
    }
}
