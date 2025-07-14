use crate::builder::SyncWorkerMode;
use crate::client::DeviceSync;
use crate::groups::device_sync::worker::SyncMetric;
use crate::groups::device_sync::DeviceSyncClient;
use crate::subscriptions::{LocalEvents, SyncWorkerEvent};
use crate::utils::VersionInfo;
use crate::worker::metrics::WorkerMetrics;
use crate::worker::WorkerRunner;
use crate::GroupCommitLock;
use crate::{
    identity::{Identity, IdentityError},
    mutex_registry::MutexRegistry,
};
use openmls_traits::storage::StorageProvider;
use openmls_traits::storage::CURRENT_VERSION;
use std::sync::Arc;
use tokio::sync::broadcast;
use xmtp_api::{ApiClientWrapper, XmtpApi};
use xmtp_common::types::InstallationId;
use xmtp_db::sql_key_store::{SqlKeyStore, SqlKeyStoreError};
use xmtp_db::XmtpMlsStorageProvider;
use xmtp_db::{prelude::*, xmtp_openmls_provider::XmtpOpenMlsProvider};
use xmtp_db::{ConnectionExt, DbConnection, MlsProviderExt, XmtpDb};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_id::{associations::builder::SignatureRequest, InboxIdRef};

pub trait XmtpSharedContext
where
    Self: Send + Sync + Sized + Clone,
{
    type Db: XmtpDb;
    type ApiClient: XmtpApi;
    type MlsStorage: Send + Sync + XmtpMlsStorageProvider;

    fn context_ref(&self)
        -> &Arc<XmtpMlsLocalContext<Self::ApiClient, Self::Db, Self::MlsStorage>>;

    fn db(&self) -> <Self::Db as XmtpDb>::DbQuery {
        self.context_ref().db()
    }

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        self.context_ref().api()
    }

    fn scw_verifier(&self) -> Arc<Box<dyn SmartContractSignatureVerifier>> {
        self.context_ref().scw_verifier()
    }

    fn device_sync(&self) -> &DeviceSync {
        &self.context_ref().device_sync
    }

    fn device_sync_server_url(&self) -> Option<&String> {
        self.context_ref().device_sync_server_url()
    }

    fn device_sync_worker_enabled(&self) -> bool {
        self.context_ref().device_sync_worker_enabled()
    }

    /// Creates a new MLS Provider
    fn mls_provider(&self) -> XmtpOpenMlsProvider<Self::MlsStorage> {
        XmtpOpenMlsProvider::new(&self.context_ref().mls_storage.clone())
    }

    /// a reference to the MLS Storage Type
    /// This can be related to 'db()' but may also be separate
    fn mls_storage(&self) -> &Self::MlsStorage {
        &self.context_ref().mls_storage
    }

    fn signature_request(&self) -> Option<SignatureRequest> {
        self.context_ref().signature_request()
    }

    fn identity(&self) -> &Identity {
        self.context_ref().identity()
    }

    fn inbox_id(&self) -> InboxIdRef<'_> {
        self.context_ref().inbox_id()
    }

    fn installation_id(&self) -> InstallationId {
        self.context_ref().installation_id()
    }

    fn version_info(&self) -> &VersionInfo {
        self.context_ref().version_info()
    }

    fn worker_events(&self) -> &broadcast::Sender<SyncWorkerEvent> {
        self.context_ref().worker_events
    }

    fn local_events(&self) -> &broadcast::Sender<LocalEvents> {
        self.context_ref().local_events()
    }

    fn mls_commit_lock(&self) -> &Arc<GroupCommitLock> {
        self.context_ref().mls_commit_lock()
    }
}

impl<XApiClient, XDb, XMls> XmtpSharedContext for Arc<XmtpMlsLocalContext<XApiClient, XDb, XMls>>
where
    XApiClient: XmtpApi,
    XDb: XmtpDb,
    XMls: Send + Sync + XmtpMlsStorageProvider,
{
    type Db = XDb;
    type ApiClient = XApiClient;
    type MlsStorage = XMls;

    fn context_ref(
        &self,
    ) -> &Arc<XmtpMlsLocalContext<Self::ApiClient, Self::Db, Self::MlsStorage>> {
        self
    }
}

impl<T> XmtpSharedContext for &T
where
    T: ?Sized + XmtpSharedContext,
{
    type Db = <T as XmtpSharedContext>::Db;
    type ApiClient = <T as XmtpSharedContext>::ApiClient;
    type MlsStorage = <T as XmtpSharedContext>::MlsStorage;

    fn context_ref(
        &self,
    ) -> &Arc<XmtpMlsLocalContext<Self::ApiClient, Self::Db, Self::MlsStorage>> {
        <T as XmtpSharedContext>::context_ref(self)
    }
}

/// The local context a XMTP MLS needs to function:
/// - Sqlite Database
/// - Identity for the User
pub struct XmtpMlsLocalContext<ApiClient, Db, S> {
    /// XMTP Identity
    pub(crate) identity: Identity,
    /// The XMTP Api Client
    pub(crate) api_client: ApiClientWrapper<ApiClient>,
    /// XMTP Local Storage
    pub(crate) store: Db,
    pub(crate) mls_storage: Arc<S>,
    pub(crate) mutexes: MutexRegistry,
    pub(crate) mls_commit_lock: Arc<GroupCommitLock>,
    pub(crate) version_info: VersionInfo,
    pub(crate) local_events: broadcast::Sender<LocalEvents>,
    pub(crate) worker_events: broadcast::Sender<SyncWorkerEvent>,
    pub(crate) scw_verifier: Arc<Box<dyn SmartContractSignatureVerifier>>,
    pub(crate) device_sync: DeviceSync,
    pub(crate) workers: WorkerRunner,
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
    pub fn mls_provider(&self) -> XmtpOpenMlsProvider<S> {
        XmtpOpenMlsProvider::new(self.mls_storage.clone())
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
    pub fn device_sync_client(
        self: &Arc<XmtpMlsLocalContext<ApiClient, Db, S>>,
    ) -> DeviceSyncClient<Self> {
        let metrics = self.sync_metrics();
        DeviceSyncClient::new(self, metrics.unwrap_or_default())
    }
}

impl<ApiClient, Db, S> XmtpMlsLocalContext<ApiClient, Db, S> {
    /// The installation public key is the primary identifier for an installation
    pub fn installation_public_key(&self) -> InstallationId {
        (*self.identity.installation_keys.public_bytes()).into()
    }

    /// The installation public key is the primary identifier for an installation
    pub fn installation_id(&self) -> InstallationId {
        (*self.identity.installation_keys.public_bytes()).into()
    }

    /// Get the account address of the blockchain account associated with this client
    pub fn inbox_id(&self) -> InboxIdRef<'_> {
        self.identity.inbox_id()
    }

    /// Get sequence id, may not be consistent with the backend
    pub fn inbox_sequence_id<C>(
        &self,
        conn: &DbConnection<C>,
    ) -> Result<i64, xmtp_db::ConnectionError>
    where
        C: ConnectionExt,
    {
        self.identity.sequence_id(conn)
    }

    /// Integrators should always check the `signature_request` return value of this function before calling [`register_identity`](Self::register_identity).
    /// If `signature_request` returns `None`, then the wallet signature is not required and [`register_identity`](Self::register_identity) can be called with None as an argument.
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
        self.workers.sync_metrics()
    }
}
