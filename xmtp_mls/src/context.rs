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
use std::sync::Arc;
use tokio::sync::broadcast;
use xmtp_api::{ApiClientWrapper, XmtpApi};
use xmtp_common::types::InstallationId;
use xmtp_db::xmtp_openmls_provider::XmtpOpenMlsProvider;
use xmtp_db::{ConnectionExt, DbConnection, XmtpDb};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_id::{associations::builder::SignatureRequest, InboxIdRef};

pub trait XmtpSharedContext: Sized {
    type Db: XmtpDb;
    type ApiClient: XmtpApi;
    fn context_ref(&self) -> &Arc<XmtpMlsLocalContext<Self::ApiClient, Self::Db>>;
}

impl<XApiClient, XDb> XmtpSharedContext for Arc<XmtpMlsLocalContext<XApiClient, XDb>>
where
    XApiClient: XmtpApi,
    XDb: XmtpDb,
{
    type Db = XDb;

    type ApiClient = XApiClient;

    fn context_ref(&self) -> &Arc<XmtpMlsLocalContext<Self::ApiClient, Self::Db>> {
        self
    }
}

impl<XApiClient, XDb> XmtpSharedContext for &Arc<XmtpMlsLocalContext<XApiClient, XDb>>
where
    XApiClient: XmtpApi,
    XDb: XmtpDb,
{
    type Db = XDb;

    type ApiClient = XApiClient;

    fn context_ref(&self) -> &Arc<XmtpMlsLocalContext<Self::ApiClient, Self::Db>> {
        self
    }
}

pub trait XmtpContextProvider: Sized {
    type Db: XmtpDb;
    type ApiClient: XmtpApi;

    fn context_ref(&self) -> &XmtpMlsLocalContext<Self::ApiClient, Self::Db>;

    fn db(&self) -> DbConnection<<Self::Db as XmtpDb>::Connection>;

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient>;

    fn mls_provider(&self) -> XmtpOpenMlsProvider<<Self::Db as XmtpDb>::Connection> {
        self.db().into()
    }

    fn identity(&self) -> &Identity;

    fn installation_id(&self) -> InstallationId {
        (*self.identity().installation_keys.public_bytes()).into()
    }

    fn inbox_id(&self) -> InboxIdRef<'_> {
        self.identity().inbox_id()
    }

    fn version_info(&self) -> &VersionInfo;

    fn local_events(&self) -> &broadcast::Sender<LocalEvents>;

    fn worker_events(&self) -> &broadcast::Sender<SyncWorkerEvent>;
}

impl<XApiClient, XDb> XmtpContextProvider for XmtpMlsLocalContext<XApiClient, XDb>
where
    XApiClient: XmtpApi,
    XDb: XmtpDb,
{
    type Db = XDb;
    type ApiClient = XApiClient;

    fn db(&self) -> DbConnection<<Self::Db as XmtpDb>::Connection> {
        XmtpMlsLocalContext::<XApiClient, XDb>::db(self)
    }

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        &self.api_client
    }

    fn context_ref(&self) -> &XmtpMlsLocalContext<Self::ApiClient, Self::Db> {
        self
    }

    fn version_info(&self) -> &VersionInfo {
        &self.version_info
    }

    fn identity(&self) -> &Identity {
        &self.identity
    }

    fn local_events(&self) -> &broadcast::Sender<LocalEvents> {
        &self.local_events
    }

    fn worker_events(&self) -> &broadcast::Sender<SyncWorkerEvent> {
        &self.worker_events
    }
}

impl<T> XmtpContextProvider for Arc<T>
where
    T: XmtpContextProvider,
{
    type Db = <T as XmtpContextProvider>::Db;
    type ApiClient = <T as XmtpContextProvider>::ApiClient;

    fn db(&self) -> DbConnection<<Self::Db as XmtpDb>::Connection> {
        <T as XmtpContextProvider>::db(&**self)
    }

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        <T as XmtpContextProvider>::api(&**self)
    }

    fn context_ref(&self) -> &XmtpMlsLocalContext<Self::ApiClient, Self::Db> {
        <T as XmtpContextProvider>::context_ref(&**self)
    }

    fn version_info(&self) -> &VersionInfo {
        <T as XmtpContextProvider>::version_info(&**self)
    }

    fn identity(&self) -> &Identity {
        <T as XmtpContextProvider>::identity(&**self)
    }

    fn local_events(&self) -> &broadcast::Sender<LocalEvents> {
        <T as XmtpContextProvider>::local_events(&**self)
    }

    fn worker_events(&self) -> &broadcast::Sender<SyncWorkerEvent> {
        <T as XmtpContextProvider>::worker_events(&**self)
    }
}

impl<T> XmtpContextProvider for &T
where
    T: XmtpContextProvider,
{
    type Db = <T as XmtpContextProvider>::Db;
    type ApiClient = <T as XmtpContextProvider>::ApiClient;

    fn db(&self) -> DbConnection<<Self::Db as XmtpDb>::Connection> {
        <T as XmtpContextProvider>::db(*self)
    }

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        <T as XmtpContextProvider>::api(*self)
    }

    fn context_ref(&self) -> &XmtpMlsLocalContext<Self::ApiClient, Self::Db> {
        <T as XmtpContextProvider>::context_ref(*self)
    }

    fn version_info(&self) -> &VersionInfo {
        <T as XmtpContextProvider>::version_info(*self)
    }

    fn identity(&self) -> &Identity {
        <T as XmtpContextProvider>::identity(*self)
    }

    fn local_events(&self) -> &broadcast::Sender<LocalEvents> {
        <T as XmtpContextProvider>::local_events(*self)
    }

    fn worker_events(&self) -> &broadcast::Sender<SyncWorkerEvent> {
        <T as XmtpContextProvider>::worker_events(*self)
    }
}

/// The local context a XMTP MLS needs to function:
/// - Sqlite Database
/// - Identity for the User
pub struct XmtpMlsLocalContext<ApiClient, Db = xmtp_db::DefaultDatabase> {
    /// XMTP Identity
    pub(crate) identity: Identity,
    /// The XMTP Api Client
    pub(crate) api_client: ApiClientWrapper<ApiClient>,
    /// XMTP Local Storage
    pub(crate) store: Db,
    pub(crate) mutexes: MutexRegistry,
    pub(crate) mls_commit_lock: Arc<GroupCommitLock>,
    pub(crate) version_info: VersionInfo,
    pub(crate) local_events: broadcast::Sender<LocalEvents>,
    pub(crate) worker_events: broadcast::Sender<SyncWorkerEvent>,
    pub(crate) scw_verifier: Arc<Box<dyn SmartContractSignatureVerifier>>,
    pub(crate) device_sync: DeviceSync,
    pub(crate) workers: WorkerRunner,
}

impl<ApiClient, Db> XmtpMlsLocalContext<ApiClient, Db>
where
    Db: XmtpDb,
    ApiClient: XmtpApi,
{
    pub fn new(
        identity: Identity,
        api_client: ApiClientWrapper<ApiClient>,
        db: Db,
        scw_signature_verifier: impl SmartContractSignatureVerifier + 'static,
    ) -> Self {
        let (local_event_sender, _) = broadcast::channel(100);
        let (worker_event_sender, _) = broadcast::channel(100);

        Self {
            identity,
            api_client,
            store: db,
            mutexes: MutexRegistry::new(),
            mls_commit_lock: Arc::new(GroupCommitLock::default()),
            version_info: VersionInfo::default(), // or however you construct it
            local_events: local_event_sender,
            worker_events: worker_event_sender,
            scw_verifier: Arc::new(Box::new(scw_signature_verifier)),
            device_sync: DeviceSync {
                server_url: None,
                mode: SyncWorkerMode::Disabled,
            },
            workers: WorkerRunner::default(),
        }
    }

    /// get a reference to the monolithic Database object where
    /// higher-level queries are defined
    pub fn db(&self) -> DbConnection<<Db as XmtpDb>::Connection> {
        self.store.db()
    }

    /// Pulls a new database connection and creates a new provider
    pub fn mls_provider(&self) -> XmtpOpenMlsProvider<<Db as XmtpDb>::Connection> {
        self.db().into()
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
        self: &Arc<XmtpMlsLocalContext<ApiClient, Db>>,
    ) -> DeviceSyncClient<ApiClient, Db> {
        let metrics = self.sync_metrics();
        DeviceSyncClient::new(self, metrics.unwrap_or_default())
    }
}

impl<ApiClient, Db> XmtpMlsLocalContext<ApiClient, Db> {
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
