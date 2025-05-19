use crate::builder::SyncWorkerMode;
use crate::client::DeviceSync;
use crate::subscriptions::LocalEvents;
use crate::utils::VersionInfo;
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

    fn version_info(&self) -> &VersionInfo;

    fn local_events(&self) -> &broadcast::Sender<LocalEvents>;
}

impl<XApiClient, XDb> XmtpContextProvider for XmtpMlsLocalContext<XApiClient, XDb>
where
    XApiClient: XmtpApi,
    XDb: XmtpDb,
{
    type Db = XDb;
    type ApiClient = XApiClient;

    fn db(&self) -> DbConnection<<Self::Db as XmtpDb>::Connection> {
        todo!()
    }

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        todo!()
    }

    fn context_ref(&self) -> &XmtpMlsLocalContext<Self::ApiClient, Self::Db> {
        &self
    }

    fn version_info(&self) -> &VersionInfo {
        &self.version_info
    }

    fn identity(&self) -> &Identity {
        todo!()
    }

    fn local_events(&self) -> &broadcast::Sender<LocalEvents> {
        todo!()
    }
}

impl<T> XmtpContextProvider for Arc<T>
where
    T: XmtpContextProvider,
{
    type Db = <T as XmtpContextProvider>::Db;
    type ApiClient = <T as XmtpContextProvider>::ApiClient;

    fn db(&self) -> DbConnection<<Self::Db as XmtpDb>::Connection> {
        todo!()
    }

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        todo!()
    }

    fn context_ref(&self) -> &XmtpMlsLocalContext<Self::ApiClient, Self::Db> {
        todo!()
    }

    fn version_info(&self) -> &VersionInfo {
        todo!()
    }
    fn identity(&self) -> &Identity {
        todo!()
    }

    fn local_events(&self) -> &broadcast::Sender<LocalEvents> {
        todo!()
    }
}

impl<T> XmtpContextProvider for &T
where
    T: XmtpContextProvider,
{
    type Db = <T as XmtpContextProvider>::Db;
    type ApiClient = <T as XmtpContextProvider>::ApiClient;

    fn db(&self) -> DbConnection<<Self::Db as XmtpDb>::Connection> {
        todo!()
    }

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        todo!()
    }

    fn context_ref(&self) -> &XmtpMlsLocalContext<Self::ApiClient, Self::Db> {
        todo!()
    }

    fn version_info(&self) -> &VersionInfo {
        todo!()
    }
    fn identity(&self) -> &Identity {
        todo!()
    }

    fn local_events(&self) -> &broadcast::Sender<LocalEvents> {
        todo!()
    }
}

/// The local context a XMTP MLS needs to function:
/// - Sqlite Database
/// - Identity for the User
pub struct XmtpMlsLocalContext<ApiClient, Db = xmtp_db::DefaultDatabase> {
    /// XMTP Identity
    pub(crate) identity: Identity,
    /// The XMTP Api Client
    pub(crate) api_client: Arc<ApiClientWrapper<ApiClient>>,
    /// XMTP Local Storage
    pub(crate) store: Db,
    pub(crate) mutexes: MutexRegistry,
    pub(crate) mls_commit_lock: std::sync::Arc<GroupCommitLock>,
    pub(crate) version_info: Arc<VersionInfo>,
    pub(crate) local_events: broadcast::Sender<LocalEvents>,
    pub(crate) scw_verifier: Arc<Box<dyn SmartContractSignatureVerifier>>,
    pub(crate) device_sync: DeviceSync,
}

impl<ApiClient, Db> XmtpMlsLocalContext<ApiClient, Db>
where
    Db: XmtpDb,
    ApiClient: XmtpApi,
{
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
}
