use crate::GroupCommitLock;
use crate::builder::{ForkRecoveryOpts, SyncWorkerMode};
use crate::client::DeviceSync;
use crate::groups::device_sync::worker::SyncMetric;
use crate::subscriptions::{LocalEvents, SyncWorkerEvent};
use crate::tasks::TaskWorkerChannels;
use crate::utils::VersionInfo;
use crate::worker::metrics::WorkerMetrics;
use crate::worker::{DynMetrics, MetricsCasting, WorkerKind};
use crate::{
    identity::{Identity, IdentityError},
    mutex_registry::MutexRegistry,
};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use xmtp_api::{ApiClientWrapper, XmtpApi};
use xmtp_api_d14n::protocol::XmtpQuery;
use xmtp_common::{MaybeSend, MaybeSync};
use xmtp_db::XmtpDb;
use xmtp_db::XmtpMlsStorageProvider;
use xmtp_db::events::Events;
use xmtp_db::xmtp_openmls_provider::XmtpOpenMlsProviderRef;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_id::{InboxIdRef, associations::builder::SignatureRequest};
use xmtp_proto::types::InstallationId;

#[cfg(any(test, feature = "test-utils"))]
use crate::groups::device_sync::DeviceSyncClient;

#[derive(Default, Clone, Copy, Eq, PartialEq)]
pub enum ClientMode {
    #[default]
    Default,
    Notification,
}

/// The local context a XMTP MLS needs to function:
/// - Sqlite Database
/// - Identity for the User
pub struct XmtpMlsLocalContext<ApiClient, Db, S> {
    /// XMTP Identity
    pub(crate) identity: Identity,
    /// The XMTP Api Client
    pub(crate) api_client: ApiClientWrapper<ApiClient>,
    pub(crate) sync_api_client: ApiClientWrapper<ApiClient>, // sync-only channel
    /// XMTP Local Storage
    pub(crate) store: Db,
    pub(crate) mls_storage: S,
    pub(crate) mutexes: MutexRegistry,
    pub(crate) mls_commit_lock: Arc<GroupCommitLock>,
    pub(crate) version_info: VersionInfo,
    pub(crate) local_events: broadcast::Sender<LocalEvents>,
    pub(crate) worker_events: broadcast::Sender<SyncWorkerEvent>,
    pub(crate) events: broadcast::Sender<Events>,
    pub(crate) scw_verifier: Arc<Box<dyn SmartContractSignatureVerifier>>,
    pub(crate) device_sync: DeviceSync,
    pub(crate) fork_recovery_opts: ForkRecoveryOpts,
    // pub(crate) workers: Arc<WorkerRunner>,
    pub(crate) worker_metrics: Arc<Mutex<HashMap<WorkerKind, DynMetrics>>>,
    pub(crate) task_channels: TaskWorkerChannels,
    pub(crate) mode: ClientMode,
}

impl<ApiClient, Db, S> XmtpMlsLocalContext<ApiClient, Db, S>
where
    Db: XmtpDb,
    ApiClient: XmtpApi,
    S: XmtpMlsStorageProvider,
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

    pub fn device_sync_server_url(&self) -> Option<&String> {
        self.device_sync.server_url.as_ref()
    }

    pub fn device_sync_worker_enabled(&self) -> bool {
        !matches!(self.device_sync.mode, SyncWorkerMode::Disabled)
    }

    /// Reconstructs the DeviceSyncClient from the context
    /// used in tests
    #[cfg(any(test, feature = "test-utils"))]
    pub fn device_sync_client(
        self: &Arc<XmtpMlsLocalContext<ApiClient, Db, S>>,
    ) -> DeviceSyncClient<Arc<Self>>
    where
        ApiClient: XmtpQuery,
    {
        let metrics = self.sync_metrics();
        DeviceSyncClient::new(
            Arc::clone(self),
            metrics.unwrap_or(Arc::new(WorkerMetrics::new(self.installation_id()))),
        )
    }
}

impl<ApiClient, Db, S> XmtpMlsLocalContext<ApiClient, Db, S> {
    pub fn replace_mls_store<S2>(self, mls_store: S2) -> XmtpMlsLocalContext<ApiClient, Db, S2> {
        XmtpMlsLocalContext::<ApiClient, Db, S2> {
            identity: self.identity,
            api_client: self.api_client,
            sync_api_client: self.sync_api_client,
            store: self.store,
            mls_storage: mls_store,
            mutexes: self.mutexes,
            mls_commit_lock: self.mls_commit_lock,
            version_info: self.version_info,
            local_events: self.local_events,
            worker_events: self.worker_events,
            events: self.events,
            scw_verifier: self.scw_verifier,
            device_sync: self.device_sync,
            fork_recovery_opts: self.fork_recovery_opts,
            worker_metrics: self.worker_metrics,
            task_channels: self.task_channels,
            mode: self.mode,
        }
    }
}

impl<ApiClient, Db, S> XmtpMlsLocalContext<ApiClient, Db, S> {
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

    pub fn mls_commit_lock(&self) -> &Arc<GroupCommitLock> {
        &self.mls_commit_lock
    }

    pub fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>> {
        self.worker_metrics
            .lock()
            .get(&WorkerKind::DeviceSync)?
            .as_sync_metrics()
    }
}

pub trait XmtpSharedContext
where
    Self: MaybeSend + MaybeSync + Sized + Clone,
{
    type Db: XmtpDb;
    type ApiClient: XmtpApi + XmtpQuery;
    type MlsStorage: XmtpMlsStorageProvider;
    type ContextReference: MaybeSend + MaybeSync + Clone + Sized;

    fn context_ref(&self) -> &Self::ContextReference;
    fn db(&self) -> <Self::Db as XmtpDb>::DbQuery;
    fn api(&self) -> &ApiClientWrapper<Self::ApiClient>;
    fn sync_api(&self) -> &ApiClientWrapper<Self::ApiClient>;
    fn scw_verifier(&self) -> Arc<Box<dyn SmartContractSignatureVerifier>>;

    fn device_sync(&self) -> &DeviceSync;

    fn device_sync_server_url(&self) -> Option<&String> {
        self.device_sync().server_url.as_ref()
    }

    fn device_sync_worker_enabled(&self) -> bool {
        !matches!(self.device_sync().mode, SyncWorkerMode::Disabled)
    }

    fn fork_recovery_opts(&self) -> &ForkRecoveryOpts;

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
    fn worker_events(&self) -> &broadcast::Sender<SyncWorkerEvent>;
    fn local_events(&self) -> &broadcast::Sender<LocalEvents>;
    fn events(&self) -> &broadcast::Sender<Events>;
    fn task_channels(&self) -> &TaskWorkerChannels;
    fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>>;
    fn mls_commit_lock(&self) -> &Arc<GroupCommitLock>;
    fn mutexes(&self) -> &MutexRegistry;
    fn mode(&self) -> ClientMode;
}

impl<XApiClient, XDb, XMls> XmtpSharedContext for Arc<XmtpMlsLocalContext<XApiClient, XDb, XMls>>
where
    XApiClient: XmtpApi + XmtpQuery,
    XDb: XmtpDb,
    XMls: XmtpMlsStorageProvider,
{
    type Db = XDb;
    type ApiClient = XApiClient;
    type MlsStorage = XMls;
    type ContextReference = Arc<XmtpMlsLocalContext<Self::ApiClient, Self::Db, Self::MlsStorage>>;

    fn context_ref(&self) -> &Self::ContextReference {
        self
    }

    fn db(&self) -> <Self::Db as XmtpDb>::DbQuery {
        self.store.db()
    }

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        &self.api_client
    }

    fn sync_api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        &self.sync_api_client
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

    fn worker_events(&self) -> &broadcast::Sender<SyncWorkerEvent> {
        &self.worker_events
    }

    fn local_events(&self) -> &broadcast::Sender<LocalEvents> {
        &self.local_events
    }

    fn events(&self) -> &broadcast::Sender<Events> {
        &self.events
    }

    fn mls_commit_lock(&self) -> &Arc<GroupCommitLock> {
        &self.mls_commit_lock
    }

    fn task_channels(&self) -> &TaskWorkerChannels {
        &self.task_channels
    }

    fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>> {
        self.worker_metrics
            .lock()
            .get(&WorkerKind::DeviceSync)?
            .as_sync_metrics()
    }

    fn mutexes(&self) -> &MutexRegistry {
        &self.mutexes
    }

    fn mode(&self) -> ClientMode {
        self.mode
    }
}

impl<T> XmtpSharedContext for &T
where
    T: XmtpSharedContext,
{
    type Db = <T as XmtpSharedContext>::Db;
    type ApiClient = <T as XmtpSharedContext>::ApiClient;
    type MlsStorage = <T as XmtpSharedContext>::MlsStorage;
    type ContextReference = <T as XmtpSharedContext>::ContextReference;

    fn context_ref(&self) -> &Self::ContextReference {
        <T as XmtpSharedContext>::context_ref(self)
    }

    fn db(&self) -> <Self::Db as XmtpDb>::DbQuery {
        <T as XmtpSharedContext>::db(self)
    }

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        <T as XmtpSharedContext>::api(self)
    }

    fn sync_api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        <T as XmtpSharedContext>::sync_api(self)
    }

    fn scw_verifier(&self) -> Arc<Box<dyn SmartContractSignatureVerifier>> {
        <T as XmtpSharedContext>::scw_verifier(self)
    }

    fn device_sync(&self) -> &DeviceSync {
        <T as XmtpSharedContext>::device_sync(self)
    }

    fn device_sync_server_url(&self) -> Option<&String> {
        <T as XmtpSharedContext>::device_sync_server_url(self)
    }

    fn device_sync_worker_enabled(&self) -> bool {
        <T as XmtpSharedContext>::device_sync_worker_enabled(self)
    }

    fn fork_recovery_opts(&self) -> &ForkRecoveryOpts {
        <T as XmtpSharedContext>::fork_recovery_opts(self)
    }

    fn mls_storage(&self) -> &Self::MlsStorage {
        <T as XmtpSharedContext>::mls_storage(self)
    }

    fn identity(&self) -> &Identity {
        <T as XmtpSharedContext>::identity(self)
    }

    fn version_info(&self) -> &VersionInfo {
        <T as XmtpSharedContext>::version_info(self)
    }

    fn worker_events(&self) -> &broadcast::Sender<SyncWorkerEvent> {
        <T as XmtpSharedContext>::worker_events(self)
    }

    fn local_events(&self) -> &broadcast::Sender<LocalEvents> {
        <T as XmtpSharedContext>::local_events(self)
    }

    fn events(&self) -> &broadcast::Sender<Events> {
        <T as XmtpSharedContext>::events(self)
    }

    fn mls_commit_lock(&self) -> &Arc<GroupCommitLock> {
        <T as XmtpSharedContext>::mls_commit_lock(self)
    }

    fn task_channels(&self) -> &TaskWorkerChannels {
        <T as XmtpSharedContext>::task_channels(self)
    }

    fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>> {
        <T as XmtpSharedContext>::sync_metrics(self)
    }

    fn mutexes(&self) -> &MutexRegistry {
        <T as XmtpSharedContext>::mutexes(self)
    }

    fn mode(&self) -> ClientMode {
        <T as XmtpSharedContext>::mode(self)
    }
}
