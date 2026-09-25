#[cfg(test)]
use crate::GroupCommitLock;
use crate::{
    StorageError, XmtpApi,
    attachments::AttachmentRuntime,
    client::{Client, ClientError, DeviceSync},
    context::{XmtpMlsLocalContext, XmtpSharedContext},
    groups::change_callbacks::UnstableChangeCallbacks,
    identity::{Identity, IdentityStrategy},
    identity_updates::load_identity_updates,
    mutex_registry::MutexRegistry,
    server_configuration::ServerConfigurationHandle,
    utils::{VersionInfo, cleanup_duplicate_updates},
    worker::{WorkerRunner, tasks::TaskWorker},
    worker::{device_sync::worker::SyncWorker, disappearing_messages::DisappearingMessagesWorker},
};
use futures::FutureExt;
use prost::Message as _;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use xmtp_api::ApiClientWrapper;
use xmtp_api_backend::TrackedStatsClient;
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
type LocationStoreOpener<Db> =
    fn(
        crate::storage_location::ResolvedPaths,
        xmtp_db::EncryptionKey,
    ) -> xmtp_common::BoxDynFuture<'static, Result<Db, ClientBuilderError>>;

fn open_location_store(
    paths: crate::storage_location::ResolvedPaths,
    key: xmtp_db::EncryptionKey,
) -> xmtp_common::BoxDynFuture<'static, Result<xmtp_db::DefaultStore, ClientBuilderError>> {
    Box::pin(async move {
        #[cfg(not(target_arch = "wasm32"))]
        let db = {
            let parent = paths
                .db_path
                .parent()
                .ok_or(crate::storage_location::StorageLocationError::InboxId)?;
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(crate::storage_location::StorageLocationError::from)?;
            xmtp_db::NativeDb::builder()
                .persistent(paths.db_path.to_string_lossy().into_owned())
                .key(key)
                .build()?
        };
        #[cfg(target_arch = "wasm32")]
        let db = xmtp_db::WasmDb::new(&xmtp_db::StorageOption::Persistent(
            paths.db_path.to_string_lossy().into_owned(),
        ))
        .await
        .map_err(xmtp_db::StorageError::from)?;
        #[cfg(target_arch = "wasm32")]
        let _ = key;
        Ok(xmtp_db::EncryptedMessageStore::new(db)?)
    })
}

#[derive(Error, Debug, ErrorCode)]
pub enum ClientBuilderError {
    /// The deployment storage path could not be resolved or opened.
    /// May be retryable if local storage becomes available.
    #[error(transparent)]
    #[error_code("StorageLocation")]
    StorageLocation(#[from] crate::storage_location::StorageLocationError),
    /// Attachment storage could not be prepared or cleaned.
    /// May be retryable if local storage becomes available.
    #[error(transparent)]
    #[error_code("Attachment")]
    Attachment(#[from] crate::attachments::AttachmentClientError),
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
    pub(crate) deployment_recorder: Option<crate::storage_location::DeploymentRecorder>,
    pub(crate) data_location: Option<(
        crate::storage_location::StorageLocation,
        xmtp_db::EncryptionKey,
    )>,
    pub(crate) location_store_opener: Option<LocationStoreOpener<Db>>,
    pub(crate) mls_storage_factory: Option<fn(&Db) -> S>,
    pub(crate) storage_location_selected: bool,
    pub(crate) storage_location_conflict: bool,
    pub(crate) attachments_dir: Option<std::path::PathBuf>,
    pub(crate) attachment_options: xmtp_attachments::AttachmentOptions,
    pub(crate) api_client: Option<ApiClient>,
    pub(crate) identity: Option<Identity>,
    pub(crate) store: Option<Db>,
    pub(crate) identity_strategy: IdentityStrategy,
    pub(crate) scw_verifier: Option<Box<dyn SmartContractSignatureVerifier>>,
    /// Whether the app supplied its own verifier, exempt from the chain check.
    pub(crate) custom_scw_verifier: bool,
    pub(crate) device_sync_worker_mode: DeviceSyncMode,
    pub(crate) fork_recovery_opts: Option<ForkRecoveryOpts>,
    /// Unstable: group-change callbacks the host registered at construction.
    pub(crate) change_callbacks: UnstableChangeCallbacks,
    pub(crate) stream_policy: crate::subscriptions::policy::StreamPolicy,
    pub(crate) incoming_factory:
        Option<Arc<dyn crate::subscriptions::incoming::SubscriptionFactory>>,
    pub(crate) version_info: VersionInfo,
    pub(crate) allow_offline: bool,
    pub(crate) disable_commit_log_worker: bool,
    pub(crate) mls_storage: Option<S>,
    pub(crate) disable_workers: bool,
    pub(crate) worker_config: crate::worker::WorkerConfig,
    /// A snapshot supplied by the caller. When present the client
    /// never fetches, stores, refreshes, or checks the identifier. Rust tests
    /// only; not exposed through the bindings.
    pub(crate) config_provider: Option<Arc<dyn xmtp_configuration::ConfigProvider>>,
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
    /// Override internal limits for controlled tests.
    #[cfg(test)]
    pub(crate) fn stream_policy(
        mut self,
        settings: crate::subscriptions::policy::StreamPolicy,
    ) -> Self {
        self.stream_policy = settings;
        self
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn new(identity_strategy: IdentityStrategy) -> Self {
        Self {
            deployment_recorder: None,
            data_location: None,
            location_store_opener: None,
            mls_storage_factory: None,
            storage_location_selected: false,
            storage_location_conflict: false,
            attachments_dir: None,
            attachment_options: xmtp_attachments::AttachmentOptions::default(),
            identity_strategy,
            api_client: None,
            identity: None,
            store: None,
            scw_verifier: None,
            custom_scw_verifier: false,
            device_sync_worker_mode: DeviceSyncMode::Enabled,
            fork_recovery_opts: None,
            change_callbacks: UnstableChangeCallbacks::default(),
            stream_policy: crate::subscriptions::policy::StreamPolicy::default(),
            incoming_factory: None,
            version_info: VersionInfo::default(),
            allow_offline: false,
            disable_commit_log_worker: false,
            mls_storage: None,
            disable_workers: false,
            worker_config: crate::worker::WorkerConfig::default(),
            config_provider: None,
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
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
            deployment_recorder: None,
            data_location: None,
            location_store_opener: None,
            mls_storage_factory: None,
            storage_location_selected: false,
            storage_location_conflict: false,
            attachments_dir: client.context.attachments.dir.clone(),
            attachment_options: client.context.attachments.options.clone(),
            api_client: Some(cloned_api),
            identity: Some(client.context.identity.clone()),
            store: Some(client.context.store.clone()),
            identity_strategy: IdentityStrategy::CachedOnly,
            scw_verifier: Some(Box::new(client.context.scw_verifier.clone())),
            custom_scw_verifier: false,
            device_sync_worker_mode: client.context.device_sync.mode,
            fork_recovery_opts: Some(client.context.fork_recovery_opts.clone()),
            change_callbacks: client.context.change_callbacks.clone(),
            stream_policy: client.context.incoming_runtime.policy().clone(),
            incoming_factory: client.context.incoming_runtime.factory.clone(),
            version_info: client.context.version_info.clone(),
            allow_offline: false,
            disable_commit_log_worker: false,
            mls_storage: Some(client.context.mls_storage.clone()),
            disable_workers: false,
            worker_config: client.context.worker_config.clone(),
            config_provider: None,
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
        ApiClient: XmtpApi + 'static,
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
        ApiClient: XmtpApi + 'static,
        Db: xmtp_db::XmtpDb + 'static,
        S: XmtpMlsStorageProvider + 'static,
    {
        let ClientBuilder {
            mut deployment_recorder,
            data_location,
            location_store_opener,
            mls_storage_factory,
            storage_location_conflict,
            mut attachments_dir,
            attachment_options,
            mut api_client,
            identity,
            mut store,
            identity_strategy,
            mut scw_verifier,
            custom_scw_verifier,

            device_sync_worker_mode,
            fork_recovery_opts,
            change_callbacks,
            stream_policy,
            incoming_factory,
            version_info,
            allow_offline,
            disable_commit_log_worker,
            mut mls_storage,
            disable_workers,
            worker_config,
            config_provider,
            ..
        } = self;

        if storage_location_conflict {
            return Err(crate::storage_location::StorageLocationError::ConflictingStore.into());
        }

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

        let mut api_client = ApiClientWrapper::new(api_client, Retry::default());
        if let Some((location, key)) = data_location {
            use crate::storage_location::StorageLocationError;
            let inbox_id = identity_strategy
                .inbox_id()
                .ok_or(StorageLocationError::InboxId)?;
            let backend_url = api_client.backend_url().unwrap_or_default().to_owned();
            let recorder = location.recorder(&backend_url);
            let recorded = match &recorder {
                Some(recorder) => recorder.lookup().await?,
                None => None,
            };
            let mut fetched = None;
            let paths = if let Some(identifier) = recorded {
                location.resolve_identifier(inbox_id, &identifier)?
            } else if matches!(
                location,
                crate::storage_location::StorageLocation::Explicit { .. }
            ) {
                location.resolve_identifier(inbox_id, "")?
            } else {
                if allow_offline {
                    return Err(StorageLocationError::OfflineMissingDeployment.into());
                }
                let response = api_client.get_configuration().await.map_err(|error| {
                    ClientBuilderError::ClientError(ClientError::ConfigurationUnavailable(
                        Box::new(crate::server_configuration::ConfigurationFetchError::Api(
                            error,
                        )),
                    ))
                })?;
                let configuration =
                    crate::server_configuration::validated(&response).map_err(ClientError::from)?;
                let paths = location.resolve_identifier(inbox_id, &configuration.identifier)?;
                fetched = Some((configuration.identifier, response));
                paths
            };
            let opener = location_store_opener.ok_or(StorageLocationError::ConflictingStore)?;
            let opened = opener(paths.clone(), key).await?;
            if let Some((identifier, response)) = fetched {
                opened.db().store_server_configuration(
                    &identifier,
                    backend_url.trim_end_matches('/'),
                    &response.encode_to_vec(),
                    xmtp_common::time::now_ns(),
                )?;
                if let Some(recorder) = &recorder {
                    recorder.record(&identifier).await?;
                }
            }
            store = Some(opened);
            attachments_dir = Some(paths.attachments_dir);
            deployment_recorder = recorder;
        }
        let store = store
            .take()
            .ok_or(ClientBuilderError::MissingParameter { parameter: "store" })?;
        let mls_storage = mls_storage
            .take()
            .or_else(|| mls_storage_factory.map(|factory| factory(&store)))
            .ok_or(ClientBuilderError::MissingParameter {
                parameter: "mls_storage",
            })?;

        let conn = store.db();

        // The configuration is resolved before any identity
        // work, so a deployment that refuses this client refuses it before the
        // database gains an identity. A caller-supplied provider
        // short-circuits every network and database path here, which is what
        // keeps `build_offline` free of a pending future.
        let has_config_provider = config_provider.is_some();
        let server_configuration = match config_provider {
            Some(provider) => ServerConfigurationHandle::new(provider),
            None => crate::server_configuration::resolve(&api_client, &conn, allow_offline).await?,
        };

        let server_configuration = server_configuration.with_chain_restriction(custom_scw_verifier);
        if let Some(recorder) = deployment_recorder {
            if !has_config_provider && !server_configuration.configuration().identifier.is_empty() {
                recorder
                    .record(&server_configuration.configuration().identifier)
                    .await?;
            }
            server_configuration.set_deployment_recorder(recorder);
        }

        // A deployment that requires a newer client refuses this build,
        // whether the snapshot came from the backend or from a provider.
        crate::server_configuration::check_minimum_version(
            server_configuration.configuration(),
            version_info.pkg_semver().semver(),
        )?;

        // A deployment that requires a credential refuses a client
        // that has no way to produce one.
        let configuration = server_configuration.configuration();
        // implements: CONF-051
        if configuration.auth.enabled && !api_client.has_credential_source() {
            return Err(ClientBuilderError::ClientError(ClientError::AuthRequired {
                required_scopes: configuration.auth.required_scopes.clone(),
            }));
        }

        // Install the snapshot before any request is made,
        // so even the identity work below chunks and pre-validates against the
        // shapes this deployment publishes. The transport is told separately,
        // because stream and interest-update chunking happens below the
        // wrapper and never sees the wrapper's copy.
        let snapshot = Arc::new(configuration.clone());
        api_client
            .api_client
            .set_limits(Arc::new(snapshot.limits.clone()));
        api_client.set_configuration(snapshot);

        let mut identity = if let Some(identity) = identity {
            identity
        } else {
            identity_strategy
                .initialize_identity(&api_client, &mls_storage, &scw_verifier)
                .await?
        };

        // The registration request is handed to the app to
        // sign, so bind it to the chains the deployment accepts before it
        // leaves the client.
        if let Some(request) = identity.signature_request.as_mut() {
            server_configuration.restrict(request);
        }

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
        // when it registers worker subscriptions. The old fields keep working;
        // they just write here.
        let mut worker_config = worker_config;
        if disable_workers {
            // Disable optional workers. The HMAC epoch observer still runs for
            // every open client.
            for kind in [
                crate::worker::WorkerKind::DeviceSync,
                crate::worker::WorkerKind::DisappearingMessages,
                crate::worker::WorkerKind::CommitLog,
                crate::worker::WorkerKind::TaskRunner,
                crate::worker::WorkerKind::ConfigurationRefresh,
                crate::worker::WorkerKind::AttachmentCleanup,
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

        let events = xmtp_events::EventBus::new();
        let events_for_lifetime = events.clone();
        let public_event_writer: Arc<dyn xmtp_events::EventWriter<()>> =
            Arc::new(xmtp_events::PublicBusWriter::new(&events));
        server_configuration.set_event_writer(public_event_writer.clone());
        api_client
            .api_client
            .register_client_event_writer(&public_event_writer);
        let mut workers = WorkerRunner::new();
        let attachments =
            Arc::new(AttachmentRuntime::new(attachments_dir, attachment_options).await?);
        let context = Arc::new(XmtpMlsLocalContext {
            attachments,
            identity,
            mls_storage,
            store,
            api_client,
            version_info,
            server_configuration,
            scw_verifier: Arc::new(scw_verifier),
            mutexes: MutexRegistry::new(),
            #[cfg(test)]
            mls_commit_lock: Arc::new(GroupCommitLock::new()),
            events,
            public_event_writer,
            registration_event_pending: Arc::new(AtomicBool::new(false)),
            device_sync: DeviceSync {
                mode: device_sync_worker_mode,
            },
            fork_recovery_opts: fork_recovery_opts.unwrap_or_default(),
            change_callbacks,
            incoming_runtime: Arc::new(crate::subscriptions::incoming::IncomingRuntime::new(
                stream_policy,
                incoming_factory,
            )),
            worker_config,

            worker_metrics: workers.metrics().clone(),
            task_channels: workers.task_channels().clone(),
            cancellation_token: CancellationToken::new(),
            shutdown_complete: Arc::new(AtomicBool::new(false)),
            delivery_owner: Default::default(),
            identity_resolutions: Default::default(),
        });

        // register workers
        if let Err(error) = context.attachments.sweep(&context).await {
            tracing::warn!(%error, "attachment cleanup failed during client build");
        }
        if let Err(error) = context.attachments.reconcile(&context).await {
            tracing::warn!(%error, "attachment reconciliation failed during client build");
        }
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
            // The deployment decides whether the commit log runs,
            // falling back to the compiled default when it says nothing.
            if enabled(WorkerKind::CommitLog)
                && context
                    .server_configuration()
                    .configuration()
                    .mls
                    .commit_log_enabled()
                && !disable_commit_log_worker
            {
                workers.register_new_worker::<
                crate::groups::commit_log::CommitLogWorker<ContextParts<ApiClient, S, Db>>,
                _,
                >(context.clone());
            }
            // Only a client that reads its configuration refreshes it.
            // A caller-supplied provider owns its own values.
            if enabled(WorkerKind::ConfigurationRefresh) && !has_config_provider {
                workers
                    .register_new_worker::<crate::server_configuration::worker::ConfigurationWorker<
                        ContextParts<ApiClient, S, Db>,
                    >, _>(context.clone());
            }
            if enabled(WorkerKind::TaskRunner) {
                workers.register_new_worker::<TaskWorker<ContextParts<ApiClient, S, Db>>, _>(
                    context.clone(),
                );
                // KP maintenance is deliberately coupled to the TaskRunner (no
                // standalone fallback): disabling the TaskRunner — per-kind via
                // WorkerConfig or globally via disable_workers — disables KP
                // rotation/deletion with it. Seeding failure is fatal to the
                // build: the DB was already opened/migrated above, so an error
                // here means it is broken; building a client whose critical
                // maintenance silently never got seeded would be worse.
                crate::worker::key_package_maintenance::seed_and_reconcile_kp_tasks(&context)?;
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
            }
            if enabled(WorkerKind::AttachmentCleanup) && context.attachments.store.is_some() {
                workers.register_new_worker::<crate::attachments::cleanup::AttachmentCleanup<
                    ContextParts<ApiClient, S, Db>,
                >, _>(context.clone());
            }
        }

        // Every open client observes HMAC epoch changes, including clients
        // that disable the other background workers.
        workers.register_new_worker::<
            crate::worker::hmac_epoch::HmacEpochWorker<ContextParts<ApiClient, S, Db>>,
            _,
        >(context.clone());

        let workers = Arc::new(workers);

        workers.spawn(context.clone());

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
            workers,
            app_lifetime: Arc::new(crate::client::AppLifetime {
                events: events_for_lifetime,
            }),
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

    pub fn attachments_dir(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.storage_location_conflict |= self.storage_location_selected;
        self.attachments_dir = Some(path.into());
        self
    }

    pub fn attachment_options(mut self, options: xmtp_attachments::AttachmentOptions) -> Self {
        self.attachment_options = options;
        self
    }

    /// Save a deployment-scoped location. `build` resolves it after all options are set.
    pub async fn data_location(
        self,
        location: crate::storage_location::StorageLocation,
        key: xmtp_db::EncryptionKey,
    ) -> Result<ClientBuilder<ApiClient, S, xmtp_db::DefaultStore>, ClientBuilderError> {
        let conflict = self.storage_location_conflict
            || self.store.is_some()
            || self.mls_storage.is_some()
            || self.mls_storage_factory.is_some()
            || self.storage_location_selected
            || self.attachments_dir.is_some();
        Ok(ClientBuilder {
            deployment_recorder: None,
            data_location: Some((location, key)),
            location_store_opener: Some(open_location_store),
            mls_storage_factory: None,
            storage_location_selected: true,
            storage_location_conflict: conflict,
            attachments_dir: self.attachments_dir,
            attachment_options: self.attachment_options,
            api_client: self.api_client,
            identity: self.identity,
            store: None,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            custom_scw_verifier: self.custom_scw_verifier,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            change_callbacks: self.change_callbacks,
            stream_policy: self.stream_policy,
            incoming_factory: self.incoming_factory,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self.mls_storage,
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
            config_provider: self.config_provider,
        })
    }

    /// Unstable: register callbacks notified when group state changes.
    ///
    /// Registration is construction-time by necessity — the changes these
    /// report arrive from the stream and sync paths, where no SDK call is on
    /// the stack. Passing [`UnstableChangeCallbacks::default`] (nothing set)
    /// is equivalent to not calling this at all.
    ///
    /// See [`crate::groups::change_callbacks`] for the delivery contract.
    pub fn unstable_change_callbacks(self, change_callbacks: UnstableChangeCallbacks) -> Self {
        Self {
            change_callbacks,
            ..self
        }
    }

    pub fn store<NewDb>(self, db: NewDb) -> ClientBuilder<ApiClient, S, NewDb> {
        ClientBuilder {
            deployment_recorder: self.deployment_recorder,
            data_location: self.data_location,
            location_store_opener: None,
            mls_storage_factory: None,
            storage_location_selected: self.storage_location_selected,
            storage_location_conflict: self.storage_location_conflict
                || self.storage_location_selected,
            store: Some(db),
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            custom_scw_verifier: self.custom_scw_verifier,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            change_callbacks: self.change_callbacks,
            stream_policy: self.stream_policy,
            incoming_factory: self.incoming_factory,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self.mls_storage,
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
            config_provider: self.config_provider,
            attachments_dir: self.attachments_dir,
            attachment_options: self.attachment_options,
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
            deployment_recorder: self.deployment_recorder,
            data_location: self.data_location,
            location_store_opener: self.location_store_opener,
            mls_storage_factory: Some(|store: &Db| SqlKeyStore::new(store.db())),
            storage_location_selected: self.storage_location_selected,
            storage_location_conflict: self.storage_location_conflict,
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            custom_scw_verifier: self.custom_scw_verifier,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            change_callbacks: self.change_callbacks,
            stream_policy: self.stream_policy,
            incoming_factory: self.incoming_factory,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self
                .store
                .as_ref()
                .map(|store| SqlKeyStore::new(store.db())),
            store: self.store,
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
            config_provider: self.config_provider,
            attachments_dir: self.attachments_dir,
            attachment_options: self.attachment_options,
        })
    }

    pub fn mls_storage<NewS>(self, mls_storage: NewS) -> ClientBuilder<ApiClient, NewS, Db> {
        ClientBuilder {
            deployment_recorder: self.deployment_recorder,
            data_location: self.data_location,
            location_store_opener: self.location_store_opener,
            mls_storage_factory: None,
            storage_location_selected: self.storage_location_selected,
            storage_location_conflict: self.storage_location_conflict,
            store: self.store,
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            custom_scw_verifier: self.custom_scw_verifier,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            change_callbacks: self.change_callbacks,
            stream_policy: self.stream_policy,
            incoming_factory: self.incoming_factory,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: Some(mls_storage),
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
            config_provider: self.config_provider,
            attachments_dir: self.attachments_dir,
            attachment_options: self.attachment_options,
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

    /// Supply the server configuration instead of reading it.
    ///
    /// With a provider in place the client never fetches, stores, refreshes, or
    /// checks the deployment identifier. Rust callers only — the bindings do
    /// not expose this.
    pub fn config_provider(
        mut self,
        provider: Arc<dyn xmtp_configuration::ConfigProvider>,
    ) -> Self {
        self.config_provider = Some(provider);
        self
    }

    /// Attach a query-only API client. Receipt uses ordered Query pages.
    /// Standard streaming clients use `api_client_with_streams` at construction.
    pub fn api_client<A>(self, api_client: A) -> ClientBuilder<A, S, Db> {
        ClientBuilder {
            deployment_recorder: self.deployment_recorder,
            data_location: self.data_location,
            location_store_opener: self.location_store_opener,
            mls_storage_factory: self.mls_storage_factory,
            storage_location_selected: self.storage_location_selected,
            storage_location_conflict: self.storage_location_conflict,
            api_client: Some(api_client),
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            custom_scw_verifier: self.custom_scw_verifier,
            store: self.store,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            change_callbacks: self.change_callbacks,
            stream_policy: self.stream_policy,
            incoming_factory: None,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self.mls_storage,
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
            config_provider: self.config_provider,
            attachments_dir: self.attachments_dir,
            attachment_options: self.attachment_options,
        }
    }

    xmtp_common::if_native! {
        /// Attach a native API client with a lazy shared Bidi receiver.
        /// No transport task or connection starts until the first receiving interest.
        pub fn api_client_with_streams<A>(self, api_client: A) -> ClientBuilder<A, S, Db>
        where
            A: xmtp_proto::api_client::XmtpMlsBidiStreams
                + crate::subscriptions::router_callbacks::ApiClientIdentity
                + Clone
                + Send
                + Sync
                + 'static,
            A::SubscribeStream: 'static,
        {
            let factory = crate::subscriptions::incoming::BidiSubscriptionFactory {
                api: api_client.clone(),
            };
            let mut builder = self.api_client(api_client);
            builder.incoming_factory = Some(Arc::new(factory));
            builder
        }
    }

    xmtp_common::if_wasm! {
        /// Attach a browser API client with a lazy static-stream receiver.
        /// The factory owns the API client, not the client context.
        pub fn api_client_with_streams<A>(self, api_client: A) -> ClientBuilder<A, S, Db>
        where
            A: xmtp_proto::api_client::XmtpMlsStreams + Clone + 'static,
        {
            let api = api_client.clone();
            let mut builder = self.api_client(api_client);
            builder.incoming_factory = Some(Arc::new(move |cursors: xmtp_proto::types::TopicCursor, limits| -> crate::subscriptions::incoming::SubscriptionFuture {
                let api = api.clone();
                Box::pin(async move {
                    api.subscribe_envelopes_with_cursors(&cursors, limits)
                        .await
                        .map(|subscription| subscription.map_error(xmtp_proto::api::NetworkError::new))
                        .map_err(xmtp_proto::api::NetworkError::new)
                })
            }));
            builder
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
            deployment_recorder: self.deployment_recorder,
            data_location: self.data_location,
            location_store_opener: self.location_store_opener,
            mls_storage_factory: self.mls_storage_factory,
            storage_location_selected: self.storage_location_selected,
            storage_location_conflict: self.storage_location_conflict,
            api_client: Some(TrackedStatsClient::new(
                self.api_client.expect("checked for none"),
            )),
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            custom_scw_verifier: self.custom_scw_verifier,
            store: self.store,

            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            change_callbacks: self.change_callbacks,
            stream_policy: self.stream_policy,
            incoming_factory: self.incoming_factory,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self.mls_storage,
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
            config_provider: self.config_provider,
            attachments_dir: self.attachments_dir,
            attachment_options: self.attachment_options,
        })
    }

    pub fn with_scw_verifier(
        self,
        verifier: impl SmartContractSignatureVerifier + 'static,
    ) -> ClientBuilder<ApiClient, S, Db> {
        ClientBuilder {
            deployment_recorder: self.deployment_recorder,
            data_location: self.data_location,
            location_store_opener: self.location_store_opener,
            mls_storage_factory: self.mls_storage_factory,
            storage_location_selected: self.storage_location_selected,
            storage_location_conflict: self.storage_location_conflict,
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: Some(Box::new(verifier)),
            custom_scw_verifier: true,
            store: self.store,

            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            change_callbacks: self.change_callbacks,
            stream_policy: self.stream_policy,
            incoming_factory: self.incoming_factory,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self.mls_storage,
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
            config_provider: self.config_provider,
            attachments_dir: self.attachments_dir,
            attachment_options: self.attachment_options,
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
            deployment_recorder: self.deployment_recorder,
            data_location: self.data_location,
            location_store_opener: self.location_store_opener,
            mls_storage_factory: self.mls_storage_factory,
            storage_location_selected: self.storage_location_selected,
            storage_location_conflict: self.storage_location_conflict,
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: Some(Box::new(ApiClientWrapper::new(api, Retry::default()))
                as Box<dyn SmartContractSignatureVerifier>),
            // The default verifier replaces the caller's verifier, so the
            // exemption from the chain restriction ends here.
            custom_scw_verifier: false,
            store: self.store,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            change_callbacks: self.change_callbacks,
            stream_policy: self.stream_policy,
            incoming_factory: self.incoming_factory,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self.mls_storage,
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
            config_provider: self.config_provider,
            attachments_dir: self.attachments_dir,
            attachment_options: self.attachment_options,
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
        assert!(
            kinds.contains(&WorkerKind::TaskRunner),
            "un-disabled worker must still be registered, got {kinds:?}"
        );
        let subscriptions = alix.client.workers.subscribed_kinds();
        assert!(!subscriptions.contains(&WorkerKind::DisappearingMessages));
        assert!(subscriptions.contains(&WorkerKind::TaskRunner));
    }

    #[xmtp_common::test(unwrap_try = true)]
    #[cfg_attr(target_arch = "wasm32", ignore)]
    async fn disabled_optional_workers_keep_the_epoch_observer() {
        tester!(alix, disable_workers);
        assert_eq!(
            alix.client.workers.registered_kinds(),
            vec![WorkerKind::HmacEpoch]
        );
        assert!(alix.client.workers.is_running());
    }
}
