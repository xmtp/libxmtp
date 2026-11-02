#[cfg(test)]
use crate::core::commit_lock::GroupCommitLock;
#[cfg(any(test, target_arch = "wasm32"))]
mod message_clock;
use crate::core::change_callbacks::UnstableChangeCallbacks;
use crate::core::config::{DeviceSync, DeviceSyncMode, ForkRecoveryOpts};
use crate::core::events::InternalEvent;
use crate::core::metrics::SyncMetric;
use crate::core::policy::StreamPolicy;
use crate::core::task_channels::TaskWorkerChannels;
use crate::core::version::VersionInfo;
use crate::server_configuration::ServerConfigurationHandle;
use crate::worker::metrics::WorkerMetrics;
use crate::worker::{SyncMetricsSlot, WorkerConfig, WorkerKind};
use crate::{
    identity::{Identity, IdentityError},
    mutex_registry::MutexRegistry,
};
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio_util::sync::CancellationToken;
use xmtp_api::{ApiClientWrapper, XmtpApi};
use xmtp_common::{MaybeSend, MaybeSync};
use xmtp_db::XmtpDb;
use xmtp_db::XmtpMlsStorageProvider;
use xmtp_db::xmtp_openmls_provider::XmtpOpenMlsProviderRef;
use xmtp_events::EventBus;
use xmtp_events::EventWriter;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_id::{InboxIdRef, associations::builder::SignatureRequest};
use xmtp_proto::types::InstallationId;

#[cfg(test)]
pub(crate) static FAIL_NEXT_DELIVERY_RELEASE: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// Counts the running host calls that can use the database.
///
/// `Client::close` cancels the client, then closes this gate and waits until
/// no call runs. Only after that does it stop the workers and disconnect the
/// database, so a call such as `reconnect_db` cannot start work that outlives
/// close. A call that is inside the gate must not wait for `Client::close` of
/// its own client, or for a worker, because close waits for that call.
/// Only request calls enter the gate. A stream or a reader must stop on
/// cancellation, or close does not finish.
#[derive(Default)]
pub struct ForegroundCalls {
    state: Mutex<ForegroundCallState>,
    idle: tokio::sync::Notify,
}

#[derive(Default)]
struct ForegroundCallState {
    closed: bool,
    running: usize,
}

/// One running call. Drop it when the call no longer uses the database.
#[must_use = "the call leaves the gate when this guard drops"]
pub struct ForegroundCall {
    calls: Arc<ForegroundCalls>,
}

impl ForegroundCalls {
    /// Start a call, or return `None` after close has begun.
    pub fn enter(self: &Arc<Self>) -> Option<ForegroundCall> {
        let mut state = self.state.lock();
        if state.closed {
            return None;
        }
        state.running += 1;
        Some(ForegroundCall {
            calls: self.clone(),
        })
    }

    /// Refuse new calls, then wait until the running calls finish.
    pub async fn close_and_wait(&self) {
        self.state.lock().closed = true;
        loop {
            let idle = self.idle.notified();
            futures::pin_mut!(idle);
            idle.as_mut().enable();
            if self.state.lock().running == 0 {
                return;
            }
            idle.await;
        }
    }
}

impl Drop for ForegroundCall {
    fn drop(&mut self) {
        let mut state = self.calls.state.lock();
        state.running -= 1;
        if state.running == 0 {
            self.calls.idle.notify_waiters();
        }
    }
}

/// The local context a XMTP MLS needs to function:
/// - Sqlite Database
/// - Identity for the User
///
/// `F` holds what the crates above core add to it.
pub struct LocalContext<ApiClient, Db, S, F> {
    /// XMTP Identity
    pub(crate) identity: Identity,
    /// The XMTP Api Client
    pub(crate) api_client: ApiClientWrapper<ApiClient>,
    /// XMTP Local Storage
    pub(crate) store: Db,
    pub(crate) mls_storage: S,
    pub(crate) mutexes: MutexRegistry,
    #[cfg(test)]
    pub(crate) mls_commit_lock: Arc<GroupCommitLock>,
    pub(crate) version_info: VersionInfo,
    /// What this deployment published about itself, resolved once at build
    /// plus the blocked connection a refresh may set.
    pub(crate) server_configuration: ServerConfigurationHandle,
    pub(crate) events: EventBus<InternalEvent>,
    pub(crate) public_event_writer: Arc<dyn EventWriter<()>>,
    /// Set by a new registration and cleared after its visibility event.
    pub(crate) registration_event_pending: Arc<AtomicBool>,
    pub(crate) delivery_owner: Arc<Mutex<Option<xmtp_db::delivery::DeliveryOwner>>>,
    pub(crate) scw_verifier: Arc<Box<dyn SmartContractSignatureVerifier>>,
    pub(crate) device_sync: DeviceSync,
    pub(crate) fork_recovery_opts: ForkRecoveryOpts,
    /// Unstable: SDK-registered notifications for group-state changes. Empty
    /// unless the host opted in at build time.
    pub(crate) change_callbacks: UnstableChangeCallbacks,
    /// The limits on incoming work; the incoming runtime holds the same policy.
    pub(crate) stream_policy: StreamPolicy,
    pub(crate) worker_config: WorkerConfig,
    // pub(crate) workers: Arc<WorkerRunner>,
    pub(crate) worker_metrics: SyncMetricsSlot,
    pub(crate) task_channels: TaskWorkerChannels,
    pub(crate) cancellation_token: CancellationToken,
    // Set only after a successful `Client::close` (workers stopped + DB
    // disconnected). The cancellation token tracks "shutdown initiated";
    // this tracks "shutdown completed cleanly" — distinct semantics so
    // a `disconnect()` failure mid-`close` doesn't silently short-circuit
    // future retries.
    pub(crate) shutdown_complete: Arc<AtomicBool>,
    pub(crate) foreground_calls: Arc<ForegroundCalls>,
    #[cfg(any(test, target_arch = "wasm32"))]
    pub(crate) message_clock: Arc<message_clock::MessageClock>,
    pub(crate) features: F,
}

/// The context of a client, with every feature the client has.
pub type XmtpMlsLocalContext<ApiClient, Db, S> =
    LocalContext<ApiClient, Db, S, crate::features::MlsFeatures>;

impl<ApiClient, Db, S, F> LocalContext<ApiClient, Db, S, F>
where
    Db: XmtpDb + 'static,
    ApiClient: XmtpApi + 'static,
    S: XmtpMlsStorageProvider + 'static,
{
    /// get a reference to the monolithic Database object where
    /// higher-level queries are defined
    pub fn db(&self) -> Db::DbQuery {
        self.store.db()
    }

    /// Creates a new MLS Provider
    pub fn mls_provider(&'_ self) -> XmtpOpenMlsProviderRef<'_, S> {
        XmtpOpenMlsProviderRef::new(&self.mls_storage)
    }

    pub fn store(&self) -> &Db {
        &self.store
    }

    pub fn scw_verifier(&self) -> &Arc<Box<dyn SmartContractSignatureVerifier>> {
        &self.scw_verifier
    }

    pub fn device_sync_worker_enabled(&self) -> bool {
        !matches!(self.device_sync.mode, DeviceSyncMode::Disabled)
    }
}

impl<ApiClient, Db, S, F> LocalContext<ApiClient, Db, S, F> {
    pub fn replace_mls_store<S2>(self, mls_store: S2) -> LocalContext<ApiClient, Db, S2, F> {
        LocalContext::<ApiClient, Db, S2, F> {
            identity: self.identity,
            api_client: self.api_client,
            store: self.store,
            mls_storage: mls_store,
            mutexes: self.mutexes,
            #[cfg(test)]
            mls_commit_lock: self.mls_commit_lock,
            version_info: self.version_info,
            server_configuration: self.server_configuration,
            events: self.events,
            public_event_writer: self.public_event_writer,
            registration_event_pending: self.registration_event_pending,
            delivery_owner: self.delivery_owner,
            scw_verifier: self.scw_verifier,
            device_sync: self.device_sync,
            fork_recovery_opts: self.fork_recovery_opts,
            change_callbacks: self.change_callbacks,
            stream_policy: self.stream_policy,
            worker_config: self.worker_config,
            worker_metrics: self.worker_metrics,
            task_channels: self.task_channels,
            cancellation_token: self.cancellation_token,
            shutdown_complete: self.shutdown_complete,
            foreground_calls: self.foreground_calls,
            #[cfg(any(test, target_arch = "wasm32"))]
            message_clock: self.message_clock,
            features: self.features,
        }
    }
}

impl<ApiClient, Db, S, F> LocalContext<ApiClient, Db, S, F> {
    /// The installation public key is the primary identifier for an installation
    pub fn installation_public_key(&self) -> InstallationId {
        (*self.identity.installation_keys.public_bytes()).into()
    }

    /// The installation public key is the primary identifier for an installation
    pub fn installation_id(&self) -> InstallationId {
        self.identity.installation_id()
    }

    /// Get the account address of the blockchain account associated with this client
    pub fn inbox_id(&self) -> InboxIdRef<'_> {
        self.identity.inbox_id()
    }

    /// Integrators should always check the `signature_request` return value of this function before calling `register_identity`.
    /// If `signature_request` returns `None`, then the wallet signature is not required and `register_identity` can be called with None as an argument.
    pub fn signature_request(&self) -> Option<SignatureRequest> {
        self.identity.signature_request()
    }

    pub fn sign_with_public_context(
        &self,
        text: impl AsRef<str>,
    ) -> Result<Vec<u8>, IdentityError> {
        self.identity.sign_with_public_context(text)
    }

    #[cfg(test)]
    pub fn mls_commit_lock(&self) -> &Arc<GroupCommitLock> {
        &self.mls_commit_lock
    }

    pub fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>> {
        self.worker_metrics.lock().clone()
    }

    pub fn cancellation_token(&self) -> &CancellationToken {
        &self.cancellation_token
    }

    pub fn shutdown_complete(&self) -> bool {
        self.shutdown_complete.load(Ordering::Acquire)
    }

    pub fn mark_shutdown_complete(&self) {
        self.shutdown_complete.store(true, Ordering::Release);
    }
}

/// The engine's context, at its old path.
pub use crate::groups::context::XmtpSharedContext;

/// The context core code runs on: storage, the API client, the identity,
/// events and the client's plain settings. `Features` is what the crates above
/// core add; each of them reads its own part through its own trait.
pub trait CoreContext
where
    Self: MaybeSend + MaybeSync + Sized + Clone,
{
    type Db: XmtpDb;
    type ApiClient: XmtpApi;
    type MlsStorage: XmtpMlsStorageProvider;
    type Features: MaybeSend + MaybeSync;
    /// Owned, cloneable handle for background work; it cannot borrow short-lived resources.
    type ContextReference: CoreContext<
            Db = Self::Db,
            ApiClient = Self::ApiClient,
            MlsStorage = Self::MlsStorage,
            Features = Self::Features,
        > + 'static;

    /// Return the handle to clone when work can outlive this context borrow.
    fn context_ref(&self) -> &Self::ContextReference;
    fn features(&self) -> &Self::Features;
    fn db(&self) -> <Self::Db as XmtpDb>::DbQuery;
    fn api(&self) -> &ApiClientWrapper<Self::ApiClient>;
    fn scw_verifier(&self) -> Arc<Box<dyn SmartContractSignatureVerifier>>;

    fn device_sync(&self) -> &DeviceSync;

    fn device_sync_worker_enabled(&self) -> bool {
        !matches!(self.device_sync().mode, DeviceSyncMode::Disabled)
    }

    fn fork_recovery_opts(&self) -> &ForkRecoveryOpts;

    fn worker_config(&self) -> &WorkerConfig;

    /// Resolve `(base, jitter)` for a worker from the configured
    /// [`WorkerConfig`], falling back to the worker's compiled-in const.
    fn worker_interval(
        &self,
        kind: WorkerKind,
        const_default: std::time::Duration,
    ) -> (std::time::Duration, std::time::Duration) {
        self.worker_config().interval(kind, const_default)
    }

    /// Creates a new MLS Provider
    fn mls_provider(&'_ self) -> XmtpOpenMlsProviderRef<'_, Self::MlsStorage> {
        XmtpOpenMlsProviderRef::new(self.mls_storage())
    }

    fn mls_storage(&self) -> &Self::MlsStorage;
    fn identity(&self) -> &Identity;

    fn signature_request(&self) -> Option<SignatureRequest> {
        self.identity().signature_request()
    }

    fn inbox_id(&self) -> InboxIdRef<'_> {
        self.identity().inbox_id()
    }

    fn installation_id(&self) -> InstallationId {
        (*self.identity().installation_keys.public_bytes()).into()
    }

    fn version_info(&self) -> &VersionInfo;
    /// The configuration snapshot every consumer reads.
    fn server_configuration(&self) -> &ServerConfigurationHandle;
    fn events(&self) -> &EventBus<InternalEvent>;
    fn registration_event_pending(&self) -> &AtomicBool;
    fn public_event_writer(&self) -> &Arc<dyn EventWriter<()>>;
    /// This context's default-consumer token; the database is the ownership authority.
    fn delivery_owner(&self) -> &Mutex<Option<xmtp_db::delivery::DeliveryOwner>>;

    /// Release this context's message consumer before disconnecting its database.
    fn close_message_delivery(&self) -> Result<(), xmtp_db::StorageError> {
        use xmtp_db::delivery::QueryDelivery;
        #[cfg(test)]
        {
            let mut fail = FAIL_NEXT_DELIVERY_RELEASE.lock();
            if fail.as_deref() == Some(self.installation_id().to_vec().as_slice()) {
                *fail = None;
                return Err(xmtp_db::StorageError::DbDeserialize);
            }
        }
        let mut registered = self.delivery_owner().lock();
        if let Some(owner) = *registered {
            self.db().release_delivery_owner(owner)?;
            *registered = None;
        }
        Ok(())
    }
    fn task_channels(&self) -> &TaskWorkerChannels;
    /// Unstable: the host's registered group-change callbacks.
    fn change_callbacks(&self) -> &UnstableChangeCallbacks;
    /// The limits on incoming work. Internal client policy.
    fn stream_policy(&self) -> &StreamPolicy;
    fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>>;
    #[cfg(test)]
    fn mls_commit_lock(&self) -> &Arc<GroupCommitLock>;
    fn mutexes(&self) -> &MutexRegistry;
    fn cancellation_token(&self) -> &CancellationToken;

    /// Returns `true` once `Client::close` has been called on this context.
    fn is_closed(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    /// Returns `true` only after `Client::close` has fully torn the client
    /// down (workers drained + DB disconnected). Distinct from [`Self::is_closed`]
    /// which fires the moment shutdown begins — this guards `close`'s
    /// idempotency check so a mid-shutdown failure stays retryable.
    fn shutdown_complete(&self) -> bool;

    fn mark_shutdown_complete(&self);

    /// Calls that `Client::close` waits for before it disconnects the database.
    fn foreground_calls(&self) -> &Arc<ForegroundCalls>;

    /// Timestamp for a newly accepted local message. WASM sends are monotonic.
    fn message_timestamp_ns(&self) -> i64;
}

impl<XApiClient, XDb, XMls, F> CoreContext for Arc<LocalContext<XApiClient, XDb, XMls, F>>
where
    XApiClient: XmtpApi + 'static,
    XDb: XmtpDb + 'static,
    XMls: XmtpMlsStorageProvider + 'static,
    F: MaybeSend + MaybeSync + 'static,
{
    type Db = XDb;
    type ApiClient = XApiClient;
    type MlsStorage = XMls;
    type Features = F;
    type ContextReference = Self;

    fn context_ref(&self) -> &Self::ContextReference {
        self
    }

    fn features(&self) -> &F {
        &self.features
    }

    fn db(&self) -> <Self::Db as XmtpDb>::DbQuery {
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

    fn worker_config(&self) -> &WorkerConfig {
        &self.worker_config
    }

    /// a reference to the MLS Storage Type
    /// This can be related to 'db()' but may also be separate
    fn mls_storage(&self) -> &Self::MlsStorage {
        &self.mls_storage
    }

    fn identity(&self) -> &Identity {
        &self.identity
    }

    fn version_info(&self) -> &VersionInfo {
        &self.version_info
    }

    fn server_configuration(&self) -> &ServerConfigurationHandle {
        &self.server_configuration
    }

    fn events(&self) -> &EventBus<InternalEvent> {
        &self.events
    }

    fn registration_event_pending(&self) -> &AtomicBool {
        &self.registration_event_pending
    }

    fn public_event_writer(&self) -> &Arc<dyn EventWriter<()>> {
        &self.public_event_writer
    }

    fn delivery_owner(&self) -> &Mutex<Option<xmtp_db::delivery::DeliveryOwner>> {
        &self.delivery_owner
    }

    #[cfg(test)]
    fn mls_commit_lock(&self) -> &Arc<GroupCommitLock> {
        &self.mls_commit_lock
    }

    fn task_channels(&self) -> &TaskWorkerChannels {
        &self.task_channels
    }

    fn change_callbacks(&self) -> &UnstableChangeCallbacks {
        &self.change_callbacks
    }

    fn stream_policy(&self) -> &StreamPolicy {
        &self.stream_policy
    }

    fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>> {
        self.worker_metrics.lock().clone()
    }

    fn mutexes(&self) -> &MutexRegistry {
        &self.mutexes
    }

    fn cancellation_token(&self) -> &CancellationToken {
        &self.cancellation_token
    }

    fn shutdown_complete(&self) -> bool {
        self.shutdown_complete.load(Ordering::Acquire)
    }

    fn mark_shutdown_complete(&self) {
        self.shutdown_complete.store(true, Ordering::Release);
    }

    fn foreground_calls(&self) -> &Arc<ForegroundCalls> {
        &self.foreground_calls
    }

    fn message_timestamp_ns(&self) -> i64 {
        #[cfg(target_arch = "wasm32")]
        {
            self.message_clock.now_ns()
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            #[cfg(test)]
            if self.message_clock.is_frozen() {
                return self.message_clock.now_ns();
            }
            xmtp_common::time::now_ns()
        }
    }
}

impl<T> CoreContext for &T
where
    T: CoreContext,
{
    type Db = <T as CoreContext>::Db;
    type ApiClient = <T as CoreContext>::ApiClient;
    type MlsStorage = <T as CoreContext>::MlsStorage;
    type Features = <T as CoreContext>::Features;
    type ContextReference = <T as CoreContext>::ContextReference;

    fn context_ref(&self) -> &Self::ContextReference {
        <T as CoreContext>::context_ref(self)
    }

    fn features(&self) -> &Self::Features {
        <T as CoreContext>::features(self)
    }

    fn db(&self) -> <Self::Db as XmtpDb>::DbQuery {
        <T as CoreContext>::db(self)
    }

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        <T as CoreContext>::api(self)
    }

    fn scw_verifier(&self) -> Arc<Box<dyn SmartContractSignatureVerifier>> {
        <T as CoreContext>::scw_verifier(self)
    }

    fn device_sync(&self) -> &DeviceSync {
        <T as CoreContext>::device_sync(self)
    }

    fn device_sync_worker_enabled(&self) -> bool {
        <T as CoreContext>::device_sync_worker_enabled(self)
    }

    fn fork_recovery_opts(&self) -> &ForkRecoveryOpts {
        <T as CoreContext>::fork_recovery_opts(self)
    }

    fn worker_config(&self) -> &WorkerConfig {
        <T as CoreContext>::worker_config(self)
    }

    fn mls_storage(&self) -> &Self::MlsStorage {
        <T as CoreContext>::mls_storage(self)
    }

    fn identity(&self) -> &Identity {
        <T as CoreContext>::identity(self)
    }

    fn version_info(&self) -> &VersionInfo {
        <T as CoreContext>::version_info(self)
    }

    fn server_configuration(&self) -> &ServerConfigurationHandle {
        <T as CoreContext>::server_configuration(self)
    }

    fn events(&self) -> &EventBus<InternalEvent> {
        <T as CoreContext>::events(self)
    }

    fn registration_event_pending(&self) -> &AtomicBool {
        <T as CoreContext>::registration_event_pending(self)
    }

    fn public_event_writer(&self) -> &Arc<dyn EventWriter<()>> {
        <T as CoreContext>::public_event_writer(self)
    }

    fn delivery_owner(&self) -> &Mutex<Option<xmtp_db::delivery::DeliveryOwner>> {
        <T as CoreContext>::delivery_owner(self)
    }

    #[cfg(test)]
    fn mls_commit_lock(&self) -> &Arc<GroupCommitLock> {
        <T as CoreContext>::mls_commit_lock(self)
    }

    fn task_channels(&self) -> &TaskWorkerChannels {
        <T as CoreContext>::task_channels(self)
    }

    fn change_callbacks(&self) -> &UnstableChangeCallbacks {
        <T as CoreContext>::change_callbacks(self)
    }

    fn stream_policy(&self) -> &StreamPolicy {
        <T as CoreContext>::stream_policy(self)
    }

    fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>> {
        <T as CoreContext>::sync_metrics(self)
    }

    fn mutexes(&self) -> &MutexRegistry {
        <T as CoreContext>::mutexes(self)
    }

    fn cancellation_token(&self) -> &CancellationToken {
        <T as CoreContext>::cancellation_token(self)
    }

    fn shutdown_complete(&self) -> bool {
        <T as CoreContext>::shutdown_complete(self)
    }

    fn mark_shutdown_complete(&self) {
        <T as CoreContext>::mark_shutdown_complete(self)
    }

    fn foreground_calls(&self) -> &Arc<ForegroundCalls> {
        <T as CoreContext>::foreground_calls(self)
    }

    fn message_timestamp_ns(&self) -> i64 {
        <T as CoreContext>::message_timestamp_ns(self)
    }
}
