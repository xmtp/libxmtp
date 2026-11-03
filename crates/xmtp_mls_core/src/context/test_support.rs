//! Contexts that tests build without the client builder: a mock context, and
//! the parts a real-backend peer supplies.

use super::*;
use crate::identity::create_credential;
use crate::worker::tasks::TaskWorkerChannels as Channels;
use xmtp_api_backend::MockBackendClient;
use xmtp_db::{
    MemoryStorage, MockXmtpDb, sql_key_store::SqlKeyStore, test_utils::MlsMemoryStorage,
};
use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;

/// A context on a mock backend and a mock database, with the feature set `F`.
pub type MockContext<F> = LocalContext<Arc<MockBackendClient>, MockXmtpDb, MlsMemoryStorage, F>;

/// An identity with a new inbox and installation, ready for use.
pub fn mock_identity() -> Identity {
    let (inbox, cred) = xmtp_id::utils::generate_inbox_id_credential();
    Identity {
        inbox_id: inbox.clone(),
        installation_keys: cred,
        credential: create_credential(inbox).expect("a credential"),
        signature_request: None,
        is_ready: AtomicBool::new(true),
    }
}

/// An identity of `inbox_id` with a new installation, ready for use.
pub fn identity_of(inbox_id: &str) -> Identity {
    Identity {
        inbox_id: inbox_id.to_string(),
        installation_keys: xmtp_cryptography::XmtpInstallationCredential::new(),
        credential: create_credential(inbox_id).expect("a credential"),
        signature_request: None,
        is_ready: AtomicBool::new(true),
    }
}

/// A context on a mock backend and a mock database.
pub fn mock_context<F: Default>() -> MockContext<F> {
    TestContextParts {
        identity: mock_identity(),
        api_client: ApiClientWrapper::new(Arc::new(MockBackendClient::new()), Default::default()),
        store: MockXmtpDb::new(),
        mls_storage: SqlKeyStore::new(MemoryStorage::new()),
        scw_verifier: Box::new(MockSmartContractSignatureVerifier::new(true)),
        server_configuration: Default::default(),
        stream_policy: Default::default(),
    }
    .into_context(F::default())
}

/// What a test supplies for a context. Everything else starts as a new
/// client's does with its workers disabled.
pub struct TestContextParts<ApiClient, Db, S> {
    /// The installation's identity.
    pub identity: Identity,
    /// The backend connection.
    pub api_client: ApiClientWrapper<ApiClient>,
    /// The database.
    pub store: Db,
    /// The MLS key store.
    pub mls_storage: S,
    /// The smart contract signature verifier.
    pub scw_verifier: Box<dyn SmartContractSignatureVerifier>,
    /// The deployment's configuration.
    pub server_configuration: ServerConfigurationHandle,
    /// The limits on incoming work.
    pub stream_policy: StreamPolicy,
}

impl<ApiClient, Db, S> TestContextParts<ApiClient, Db, S> {
    /// A context of these parts with the feature set `features`.
    pub fn into_context<F>(self, features: F) -> LocalContext<ApiClient, Db, S, F> {
        let events = EventBus::new();
        let public_event_writer: Arc<dyn EventWriter<()>> =
            Arc::new(xmtp_events::PublicBusWriter::new(&events));
        self.server_configuration
            .set_event_writer(public_event_writer.clone());
        LocalContext {
            identity: self.identity,
            api_client: self.api_client,
            store: self.store,
            mls_storage: self.mls_storage,
            mutexes: MutexRegistry::new(),
            #[cfg(any(test, feature = "test-hooks"))]
            mls_commit_lock: Default::default(),
            version_info: VersionInfo::default(),
            server_configuration: self.server_configuration,
            public_event_writer,
            events,
            registration_event_pending: Arc::new(AtomicBool::new(false)),
            delivery_owner: Default::default(),
            scw_verifier: Arc::new(self.scw_verifier),
            device_sync: DeviceSync {
                mode: DeviceSyncMode::Disabled,
            },
            fork_recovery_opts: Default::default(),
            change_callbacks: Default::default(),
            stream_policy: self.stream_policy,
            worker_config: Default::default(),
            worker_metrics: Arc::default(),
            task_channels: Channels::default(),
            cancellation_token: CancellationToken::new(),
            shutdown_complete: Arc::new(AtomicBool::new(false)),
            foreground_calls: Default::default(),
            #[cfg(any(test, feature = "test-hooks", target_arch = "wasm32"))]
            message_clock: Default::default(),
            features,
        }
    }
}

impl<F: Clone> Clone for MockContext<F> {
    fn clone(&self) -> Self {
        Self {
            identity: self.identity.clone(),
            api_client: self.api_client.clone(),
            store: self.store.clone(),
            mls_storage: self.mls_storage.clone(),
            mutexes: self.mutexes.clone(),
            #[cfg(any(test, feature = "test-hooks"))]
            mls_commit_lock: self.mls_commit_lock.clone(),
            version_info: self.version_info.clone(),
            server_configuration: self.server_configuration.clone(),
            events: self.events.clone(),
            registration_event_pending: self.registration_event_pending.clone(),
            public_event_writer: self.public_event_writer.clone(),
            scw_verifier: self.scw_verifier.clone(),
            device_sync: self.device_sync.clone(),
            fork_recovery_opts: self.fork_recovery_opts.clone(),
            change_callbacks: self.change_callbacks.clone(),
            worker_config: self.worker_config.clone(),
            task_channels: self.task_channels.clone(),
            worker_metrics: self.worker_metrics.clone(),
            cancellation_token: self.cancellation_token.clone(),
            shutdown_complete: self.shutdown_complete.clone(),
            foreground_calls: self.foreground_calls.clone(),
            #[cfg(any(test, feature = "test-hooks", target_arch = "wasm32"))]
            message_clock: self.message_clock.clone(),
            delivery_owner: self.delivery_owner.clone(),
            stream_policy: self.stream_policy.clone(),
            features: self.features.clone(),
        }
    }
}

impl<F: MaybeSend + MaybeSync + Clone + 'static> CoreContext for MockContext<F> {
    type Db = xmtp_db::MockXmtpDb;

    type ApiClient = Arc<MockBackendClient>;

    type MlsStorage = xmtp_db::test_utils::MlsMemoryStorage;
    type Features = F;
    type ContextReference = Self;

    fn context_ref(&self) -> &Self::ContextReference {
        self
    }

    fn features(&self) -> &Self::Features {
        &self.features
    }

    fn db(&self) -> <Self::Db as xmtp_db::XmtpDb>::DbQuery {
        self.store.db()
    }

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        &self.api_client
    }

    fn scw_verifier(&self) -> Arc<Box<dyn SmartContractSignatureVerifier>> {
        self.scw_verifier.clone()
    }

    fn device_sync(&self) -> &DeviceSync {
        &self.device_sync
    }

    fn fork_recovery_opts(&self) -> &ForkRecoveryOpts {
        &self.fork_recovery_opts
    }

    fn worker_config(&self) -> &crate::worker::WorkerConfig {
        &self.worker_config
    }

    fn mls_storage(&self) -> &Self::MlsStorage {
        &self.mls_storage
    }

    fn identity(&self) -> &Identity {
        &self.identity
    }

    fn version_info(&self) -> &VersionInfo {
        &self.version_info
    }

    fn server_configuration(&self) -> &crate::server_configuration::ServerConfigurationHandle {
        &self.server_configuration
    }

    fn events(&self) -> &xmtp_events::EventBus<crate::subscriptions::internal::InternalEvent> {
        &self.events
    }

    fn registration_event_pending(&self) -> &AtomicBool {
        &self.registration_event_pending
    }

    fn public_event_writer(&self) -> &Arc<dyn xmtp_events::EventWriter<()>> {
        &self.public_event_writer
    }

    #[cfg(any(test, feature = "test-hooks"))]
    fn mls_commit_lock(&self) -> &Arc<GroupCommitLock> {
        &self.mls_commit_lock
    }

    fn mutexes(&self) -> &MutexRegistry {
        &self.mutexes
    }

    fn task_channels(&self) -> &TaskWorkerChannels {
        &self.task_channels
    }

    fn change_callbacks(&self) -> &crate::groups::change_callbacks::UnstableChangeCallbacks {
        &self.change_callbacks
    }

    fn sync_metrics(&self) -> Option<Arc<crate::worker::metrics::WorkerMetrics<SyncMetric>>> {
        self.worker_metrics.lock().clone()
    }

    fn cancellation_token(&self) -> &CancellationToken {
        &self.cancellation_token
    }

    fn delivery_owner(&self) -> &parking_lot::Mutex<Option<xmtp_db::delivery::DeliveryOwner>> {
        &self.delivery_owner
    }

    fn stream_policy(&self) -> &crate::core::policy::StreamPolicy {
        &self.stream_policy
    }

    fn shutdown_complete(&self) -> bool {
        self.shutdown_complete
            .load(std::sync::atomic::Ordering::Acquire)
    }

    fn mark_shutdown_complete(&self) {
        self.shutdown_complete
            .store(true, std::sync::atomic::Ordering::Release);
    }

    fn message_timestamp_ns(&self) -> i64 {
        xmtp_common::time::now_ns()
    }

    fn foreground_calls(&self) -> &Arc<crate::context::ForegroundCalls> {
        &self.foreground_calls
    }
}
