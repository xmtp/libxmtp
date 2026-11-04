use crate::{
    StorageError, XmtpApi,
    attachments::AttachmentRuntime,
    client::{Client, ClientError, DeviceSync},
    context::{CoreContext, XmtpMlsLocalContext, XmtpSharedContext},
    groups::change_callbacks::UnstableChangeCallbacks,
    identity::{Identity, IdentityStrategy},
    identity_updates::load_identity_updates,
    mutex_registry::MutexRegistry,
    server_configuration::ServerConfigurationHandle,
    utils::VersionInfo,
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
use xmtp_common::snippet::Snippet;
use xmtp_common::{ErrorCode, Event, Retry};
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_db::{XmtpDb, sql_key_store::SqlKeyStore};
use xmtp_db::{XmtpMlsStorageProvider, prelude::*};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_macro::log_event;
use xmtp_proto::api_client::XmtpBackendClient;
use xmtp_proto::xmtp::mls::database::{
    ProcessPendingSelfRemove, Task as TaskProto, task::Task as TaskKind,
};

pub use crate::core::config::{DeviceSyncMode, ForkRecoveryOpts, ForkRecoveryPolicy};

mod build;
mod error;
mod storage;

pub use error::ClientBuilderError;
use storage::ContextParts;
#[cfg(not(target_arch = "wasm32"))]
use storage::LocationStoreOpener;
#[cfg(target_arch = "wasm32")]
use storage::LocationStoreOpener;

pub struct ClientBuilder<ApiClient, S, Db = xmtp_db::DefaultStore> {
    pub(crate) deployment_recorder: Option<crate::storage_location::DeploymentRecorder>,
    pub(crate) data_location: Option<crate::storage_location::StorageLocation>,
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
    pub(crate) stream_policy: xmtp_mls_core::policy::StreamPolicy,
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
    #[cfg(any(test, feature = "test-utils"))]
    application_components_for_test:
        Option<Vec<xmtp_configuration::ApplicationComponentDefinition>>,
}

impl Client<()> {
    /// Get the builder for this [`Client`]
    pub fn builder(strategy: IdentityStrategy) -> ClientBuilder<(), ()> {
        ClientBuilder::<(), ()>::new(strategy)
    }
}

impl<ApiClient, S, Db> ClientBuilder<ApiClient, S, Db> {
    /// Override internal limits for controlled tests.
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn stream_policy(mut self, settings: xmtp_mls_core::policy::StreamPolicy) -> Self {
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
            stream_policy: xmtp_mls_core::policy::StreamPolicy::default(),
            incoming_factory: None,
            version_info: VersionInfo::default(),
            allow_offline: false,
            disable_commit_log_worker: false,
            mls_storage: None,
            disable_workers: false,
            worker_config: crate::worker::WorkerConfig::default(),
            config_provider: None,
            #[cfg(any(test, feature = "test-utils"))]
            application_components_for_test: None,
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
        let cloned_api: ApiClient = client.context.api_client.api_client.raw_for_test().clone();
        ClientBuilder {
            deployment_recorder: None,
            data_location: None,
            location_store_opener: None,
            mls_storage_factory: None,
            storage_location_selected: false,
            storage_location_conflict: false,
            attachments_dir: client.context.features.attachments.dir().cloned(),
            attachment_options: client.context.features.attachments.options().clone(),
            api_client: Some(cloned_api),
            identity: Some(client.context.identity.clone()),
            store: Some(client.context.store.clone()),
            identity_strategy: IdentityStrategy::CachedOnly,
            scw_verifier: Some(Box::new(client.context.scw_verifier.clone())),
            custom_scw_verifier: client.context.server_configuration.custom_verifier(),
            device_sync_worker_mode: client.context.device_sync.mode,
            fork_recovery_opts: Some(client.context.fork_recovery_opts.clone()),
            change_callbacks: client.context.change_callbacks.clone(),
            stream_policy: client.context.features.incoming.policy().clone(),
            incoming_factory: client.context.features.incoming.original_factory(),
            version_info: client.context.version_info.clone(),
            allow_offline: false,
            disable_commit_log_worker: false,
            mls_storage: Some(client.context.mls_storage.clone()),
            disable_workers: false,
            worker_config: client.context.worker_config.clone(),
            config_provider: None,
            #[cfg(any(test, feature = "test-utils"))]
            application_components_for_test: None,
        }
    }
}

// TODO: the return type is temp and
// will be modified in subsequent PRs
impl<ApiClient, S, Db> ClientBuilder<ApiClient, S, Db> {
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

    pub fn with_disable_workers(mut self, disable_workers: bool) -> Self {
        self.disable_workers = disable_workers;
        self
    }

    /// The subscription factory the built client's receiver opens through,
    /// for a test that wraps or replaces it.
    #[cfg(any(test, feature = "test-utils"))]
    #[doc(hidden)]
    pub fn incoming_factory_mut(
        &mut self,
    ) -> &mut Option<Arc<dyn crate::subscriptions::incoming::SubscriptionFactory>> {
        &mut self.incoming_factory
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

    /// Replace only the application catalogue after normal configuration admission.
    /// Used by SDK metadata test fixtures.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn application_components_for_test(
        mut self,
        components: Vec<xmtp_configuration::ApplicationComponentDefinition>,
    ) -> Self {
        self.application_components_for_test = Some(components);
        self
    }

    /// Supply the server configuration instead of reading it.
    ///
    /// With a provider in place the client does not fetch, store, or refresh
    /// the configuration. A data directory still checks its deployment identifier.
    /// Rust callers only; the bindings do not expose this.
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
            #[cfg(any(test, feature = "test-utils"))]
            application_components_for_test: self.application_components_for_test,
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
            let factory = crate::subscriptions::incoming::streaming_factory(api_client.clone());
            let mut builder = self.api_client(api_client);
            builder.incoming_factory = Some(factory);
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
            let factory = crate::subscriptions::incoming::streaming_factory(api_client.clone());
            let mut builder = self.api_client(api_client);
            builder.incoming_factory = Some(factory);
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

    /// Skip network calls when building a client. An offline build loads no
    /// identity update, so its identifier check (see [`Self::build`]) uses
    /// the newest state the database stores, which can predate a revocation.
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
            #[cfg(any(test, feature = "test-utils"))]
            application_components_for_test: self.application_components_for_test,
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
            #[cfg(any(test, feature = "test-utils"))]
            application_components_for_test: self.application_components_for_test,
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
            #[cfg(any(test, feature = "test-utils"))]
            application_components_for_test: self.application_components_for_test,
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

    #[xmtp_common::test(unwrap_try = true)]
    #[cfg_attr(target_arch = "wasm32", ignore)]
    async fn registered_worker_kinds_reads_the_registry() {
        tester!(alix, disable_workers);
        assert_eq!(
            alix.client.registered_worker_kinds(),
            vec![WorkerKind::HmacEpoch]
        );
    }
}
