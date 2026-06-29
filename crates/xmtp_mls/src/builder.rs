use crate::{
    GroupCommitLock, StorageError, XmtpApi,
    client::{Client, DeviceSync},
    context::{XmtpMlsLocalContext, XmtpSharedContext},
    identity::{Identity, IdentityStrategy},
    identity_updates::load_identity_updates,
    mutex_registry::MutexRegistry,
    utils::{VersionInfo, cleanup_duplicate_updates},
    worker::{WorkerRunner, tasks::TaskWorker},
    worker::{device_sync::worker::SyncWorker, disappearing_messages::DisappearingMessagesWorker},
};
use futures::FutureExt;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use thiserror::Error;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use xmtp_api::ApiClientWrapper;
use xmtp_api_d14n::{
    TrackedStatsClient,
    protocol::{CursorStore, XmtpQuery},
};
use xmtp_common::{ErrorCode, Event, Retry};
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_db::{DbConnection, XmtpMlsStorageProvider, prelude::*};
use xmtp_db::{XmtpDb, sql_key_store::SqlKeyStore};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_macro::log_event;
use xmtp_proto::xmtp::mls::database::{
    ProcessPendingSelfRemove, Task as TaskProto, task::Task as TaskKind,
};

type ContextParts<Api, S, Db> = Arc<XmtpMlsLocalContext<Api, Db, S>>;

#[derive(Error, Debug, ErrorCode)]
pub enum ClientBuilderError {
    #[error(transparent)]
    #[error_code(inherit)]
    AddressValidation(#[from] IdentifierValidationError),
    /// Missing parameter.
    ///
    /// Required builder parameter not provided. Not retryable.
    #[error("Missing parameter: {parameter}")]
    MissingParameter { parameter: &'static str },
    /// Client error.
    ///
    /// Client operation failed during build. May be retryable.
    #[error(transparent)]
    ClientError(#[from] crate::client::ClientError),
    /// Storage error.
    ///
    /// Storage initialization failed. Not retryable.
    #[error("Storage Error")]
    StorageError(#[from] StorageError),
    /// Identity error.
    ///
    /// Identity creation/loading failed. Not retryable.
    #[error(transparent)]
    Identity(#[from] crate::identity::IdentityError),
    /// API error.
    ///
    /// API client initialization failed. Retryable.
    #[error(transparent)]
    WrappedApiError(#[from] xmtp_api::ApiError),
    /// Group error.
    ///
    /// Group operation failed during build. Not retryable.
    #[error(transparent)]
    GroupError(#[from] Box<crate::groups::GroupError>),
    /// Device sync error.
    ///
    /// Device sync setup failed. Not retryable.
    #[error(transparent)]
    DeviceSync(#[from] Box<crate::worker::device_sync::DeviceSyncError>),
    /// Offline build failed.
    ///
    /// Builder tried to access the network in offline mode. Not retryable.
    #[error("Offline build failed, builder tried to access the network")]
    OfflineBuildFailed,
}

impl From<crate::worker::device_sync::DeviceSyncError> for ClientBuilderError {
    fn from(value: crate::worker::device_sync::DeviceSyncError) -> Self {
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
    pub(crate) device_sync_worker_mode: DeviceSyncMode,
    pub(crate) fork_recovery_opts: Option<ForkRecoveryOpts>,
    pub(crate) version_info: VersionInfo,
    pub(crate) allow_offline: bool,
    pub(crate) disable_commit_log_worker: bool,
    pub(crate) mls_storage: Option<S>,
    pub(crate) cursor_store: Option<Arc<dyn CursorStore>>,
    pub(crate) disable_workers: bool,
    pub(crate) worker_config: crate::worker::WorkerConfig,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceSyncMode {
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
            device_sync_worker_mode: DeviceSyncMode::Enabled,
            fork_recovery_opts: None,
            version_info: VersionInfo::default(),
            allow_offline: false,
            disable_commit_log_worker: false,
            mls_storage: None,
            cursor_store: None,
            disable_workers: false,
            worker_config: crate::worker::WorkerConfig::default(),
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
        ClientBuilder {
            api_client: Some(cloned_api),
            identity: Some(client.context.identity.clone()),
            store: Some(client.context.store.clone()),
            identity_strategy: IdentityStrategy::CachedOnly,
            scw_verifier: Some(Box::new(client.context.scw_verifier.clone())),
            device_sync_worker_mode: client.context.device_sync.mode,
            fork_recovery_opts: Some(client.context.fork_recovery_opts.clone()),
            version_info: client.context.version_info.clone(),
            allow_offline: false,
            disable_commit_log_worker: false,
            mls_storage: Some(client.context.mls_storage.clone()),
            cursor_store: None,
            disable_workers: false,
            worker_config: client.context.worker_config.clone(),
        }
    }
}

/// One-time backfill of `ProcessPendingSelfRemove` tasks for groups that already
/// had pending leave requests before self-removal became event-driven. Such rows
/// have no incoming LeaveRequest to re-fire, so without this they'd only be
/// processed if a new one arrived. Best-effort and idempotent (deduped per group).
///
/// TODO(#3748): removable once every client has shipped a release that enqueues
/// these tasks inline — safe to delete by the next-next stable release.
fn backfill_pending_self_remove_tasks<C>(context: &C) -> Result<(), StorageError>
where
    C: XmtpSharedContext,
{
    let db = context.db();
    for raw_id in db.get_groups_have_pending_leave_request()? {
        let Ok(group_id) = xmtp_proto::types::GroupId::try_from(raw_id.as_slice()) else {
            continue;
        };
        let now = xmtp_common::time::now_ns();
        let proto = TaskProto {
            task: Some(TaskKind::ProcessPendingSelfRemove(
                ProcessPendingSelfRemove {
                    group_id: group_id.to_vec(),
                },
            )),
        };
        let task = xmtp_db::tasks::NewTask::builder()
            .originating_message_sequence_id(0)
            .originating_message_originator_id(0)
            .created_at_ns(now)
            .next_attempt_at_ns(now)
            .build(proto)?;
        // Insert-if-absent per group: leaves any live retrying task (and its
        // backoff) untouched, only replacing dead rows. Safe to call on every
        // startup without resurrecting exhausted tasks.
        db.upsert_pending_self_remove_task(&group_id, task)?;
    }
    Ok(())
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

    #[tracing::instrument(err, skip_all, fields(operation = "mls.build_client"))]
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

            device_sync_worker_mode,
            fork_recovery_opts,
            version_info,
            allow_offline,
            disable_commit_log_worker,
            mut mls_storage,
            // cursor_store,
            disable_workers,
            worker_config,
            ..
        } = self;

        let api_client = api_client
            .take()
            .ok_or(ClientBuilderError::MissingParameter {
                parameter: "api_client",
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

        // Fold the legacy single-worker toggles into the unified enable map so
        // there is one source of truth for "is worker X enabled" that code paths
        // (e.g. the disappearing-message store site) can consult before nudging a
        // worker's channel. The old fields keep working; they just write here.
        let mut worker_config = worker_config;
        if disable_workers {
            // Global kill-switch: nothing runs, so mark every worker disabled.
            for kind in [
                crate::worker::WorkerKind::DeviceSync,
                crate::worker::WorkerKind::DisappearingMessages,
                crate::worker::WorkerKind::KeyPackageCleaner,
                crate::worker::WorkerKind::CommitLog,
                crate::worker::WorkerKind::TaskRunner,
            ] {
                worker_config.enabled.insert(kind, false);
            }
        }
        if matches!(device_sync_worker_mode, DeviceSyncMode::Disabled) {
            worker_config
                .enabled
                .entry(crate::worker::WorkerKind::DeviceSync)
                .or_insert(false);
        }
        if disable_commit_log_worker {
            worker_config
                .enabled
                .entry(crate::worker::WorkerKind::CommitLog)
                .or_insert(false);
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
                mode: device_sync_worker_mode,
            },
            fork_recovery_opts: fork_recovery_opts.unwrap_or_default(),
            worker_config,

            worker_metrics: workers.metrics().clone(),
            task_channels: workers.task_channels().clone(),
            disappearing_channels: crate::worker::disappearing_messages::DisappearingChannels::new(
            ),
            cancellation_token: CancellationToken::new(),
            shutdown_complete: Arc::new(AtomicBool::new(false)),
        });

        // register workers
        if !disable_workers {
            use crate::worker::WorkerKind;
            // One source of truth for enablement: the folded WorkerConfig map.
            let enabled = |k| context.worker_config().worker_enabled(k);

            // Keep the original `device_sync_worker_enabled()` AND-check to
            // preserve the exact legacy semantics alongside the map gate.
            if enabled(WorkerKind::DeviceSync) && context.device_sync_worker_enabled() {
                workers.register_new_worker::<SyncWorker<ContextParts<ApiClient, S, Db>>, _>(
                    context.clone(),
                );
            }
            if enabled(WorkerKind::DisappearingMessages) {
                workers
                    .register_new_worker::<DisappearingMessagesWorker<ContextParts<ApiClient, S, Db>>, _>(
                        context.clone(),
                    );
            }
            // Enable CommitLogWorker based on configuration
            if enabled(WorkerKind::CommitLog)
                && xmtp_configuration::ENABLE_COMMIT_LOG
                && !disable_commit_log_worker
            {
                workers.register_new_worker::<
                crate::groups::commit_log::CommitLogWorker<ContextParts<ApiClient, S, Db>>,
                _,
                >(context.clone());
            }
            // The recurring KP-maintenance task runs on the TaskRunner now, so the
            // TaskRunner must be registered if EITHER its own gate or the
            // KeyPackageCleaner gate is on. Seeding the KP task is gated solely on
            // KeyPackageCleaner.
            if enabled(WorkerKind::TaskRunner) || enabled(WorkerKind::KeyPackageCleaner) {
                workers.register_new_worker::<TaskWorker<ContextParts<ApiClient, S, Db>>, _>(
                    context.clone(),
                );
                // One-time backfill: pending self-removes recorded before the
                // worker became event-driven have no LeaveRequest to re-fire, so
                // seed a ProcessPendingSelfRemove task for each already-flagged
                // group. Best-effort (logged, never fails the build).
                //
                // TODO: remove this migration once all clients have shipped a
                // release that enqueues these tasks inline — safe to delete by the
                // next-next stable release.
                if let Err(e) = backfill_pending_self_remove_tasks(&context) {
                    tracing::warn!(
                        "pending-self-remove backfill failed (will rely on next sync): {e}"
                    );
                }
                // Seed the recurring KP-maintenance task only when the
                // KeyPackageCleaner gate is on. Best-effort: if it fails here the
                // TaskRunner re-seeds it on next start.
                if enabled(WorkerKind::KeyPackageCleaner)
                    && let Err(e) = context.db().ensure_kp_maintenance_task()
                {
                    tracing::warn!(
                        "kp-maintenance task seed failed (will rely on next start): {e}"
                    );
                }
            }
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
            disabled_workers = disable_workers,
        );

        let installation_id = context.installation_id();
        let client = Client {
            context,
            installation_id,
            local_events,
            workers,
        };

        // Cleanup old unstitched group updated messages.
        let conn = DbConnection::new(client.db());
        let cancel = client.context.cancellation_token().clone();
        xmtp_common::spawn(None, async move {
            tokio::select! {
                _ = cancel.cancelled() => {}
                _ = cleanup_duplicate_updates::perform(conn) => {}
            }
        });

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
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self.mls_storage,
            cursor_store: self.cursor_store,
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
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
            cursor_store: self.cursor_store,
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
        })
    }

    pub fn mls_storage<NewS>(self, mls_storage: NewS) -> ClientBuilder<ApiClient, NewS, Db> {
        ClientBuilder {
            store: self.store,
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: Some(mls_storage),
            cursor_store: self.cursor_store,
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
        }
    }

    pub fn with_disable_workers(mut self, disable_workers: bool) -> Self {
        self.disable_workers = disable_workers;
        self
    }

    pub fn with_device_sync_worker_mode(self, mode: Option<DeviceSyncMode>) -> Self {
        Self {
            device_sync_worker_mode: mode.unwrap_or(DeviceSyncMode::Enabled),
            ..self
        }
    }

    pub fn device_sync_worker_mode(self, mode: DeviceSyncMode) -> Self {
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

    /// Configure background-worker intervals, jitter, and per-worker
    /// enablement. See [`crate::worker::WorkerConfig`].
    pub fn worker_config(mut self, cfg: crate::worker::WorkerConfig) -> Self {
        self.worker_config = cfg;
        self
    }

    pub fn api_client<A>(self, api_client: A) -> ClientBuilder<A, S, Db> {
        ClientBuilder {
            api_client: Some(api_client),
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            store: self.store,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self.mls_storage,
            cursor_store: self.cursor_store,
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
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

    pub fn enable_api_stats(
        self,
    ) -> Result<ClientBuilder<TrackedStatsClient<ApiClient>, S, Db>, ClientBuilderError> {
        if self.api_client.is_none() {
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

            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self.mls_storage,
            cursor_store: self.cursor_store,
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
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

            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self.mls_storage,
            cursor_store: self.cursor_store,
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
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
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self.mls_storage,
            cursor_store: self.cursor_store,
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
        })
    }
}

#[cfg(test)]
mod worker_registration_tests {
    use crate::tester;
    use crate::worker::{WorkerConfig, WorkerKind};

    #[xmtp_common::test(unwrap_try = true)]
    #[cfg_attr(target_arch = "wasm32", ignore)]
    async fn disabled_worker_is_not_registered() {
        let mut cfg = WorkerConfig::default();
        cfg.enabled.insert(WorkerKind::DisappearingMessages, false);
        tester!(alix, worker_config: cfg);

        let kinds = alix.client.workers.registered_kinds();
        assert!(
            !kinds.contains(&WorkerKind::DisappearingMessages),
            "disabled worker must not be registered, got {kinds:?}"
        );
        // KeyPackage maintenance now runs on the TaskRunner: an enabled
        // KeyPackageCleaner gate registers the TaskRunner (no standalone
        // KeyPackageCleaner worker exists anymore).
        assert!(
            kinds.contains(&WorkerKind::TaskRunner),
            "KeyPackageCleaner-enabled must register the TaskRunner, got {kinds:?}"
        );
    }
}
