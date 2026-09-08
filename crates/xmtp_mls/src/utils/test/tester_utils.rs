#![allow(unused)]
pub use xmtp_id::utils::passkey::{PKClient, PKCredential, PasskeyUser, PkUserValidationMethod};

use super::DefaultTestClientCreator;
use super::FullXmtpClient;
use crate::worker::device_sync::{ArchiveOptions, BackupElementSelection, worker::SyncMetric};
use crate::{
    Client, MlsContext,
    builder::{ClientBuilder, DeviceSyncMode, ForkRecoveryOpts, ForkRecoveryPolicy},
    client::ClientError,
    context::XmtpSharedContext,
    groups::{GroupError, intents::UpdateGroupMembershipResult},
    identity::{Identity, IdentityStrategy, pq_key_package_references_key},
    identity_updates::load_identity_updates,
    subscriptions::SubscribeError,
    utils::{
        TestClient, TestMlsStorage, ToxicOnlyTestClientCreator, VersionInfo, register_client,
        test::identity_setup,
    },
    worker::metrics::WorkerMetrics,
};
use alloy::signers::local::PrivateKeySigner;
use diesel::{ExpressionMethods, QueryDsl, QueryableByName};
use futures::{
    AsyncReadExt, Stream, StreamExt,
    io::{BufReader, Cursor},
};
use futures_executor::block_on;
use parking_lot::Mutex;
use std::{
    ops::Deref,
    path::{Path, PathBuf},
    sync::{
        Arc, LazyLock,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::{runtime::Handle, sync::OnceCell};
use toxiproxy_rust::proxy::{Proxy, ProxyPack};
use xmtp_api::{ApiError, XmtpApi};
use xmtp_archive::{ArchiveImporter, exporter::ArchiveExporter};
use xmtp_common::StreamHandle;
use xmtp_configuration::DockerUrls;
use xmtp_configuration::{KEY_PACKAGE_ROTATION_INTERVAL_NS, LOCALHOST};
use xmtp_cryptography::{signature::SignatureError, utils::generate_local_wallet};
#[cfg(not(target_arch = "wasm32"))]
use xmtp_db::NativeDb;
#[cfg(target_arch = "wasm32")]
use xmtp_db::WasmDb;
use xmtp_db::{
    ConnectionExt, ReadOnly, TestDb, XmtpMlsStorageProvider, XmtpTestDb,
    diesel::{self, Connection, RunQueryDsl, SqliteConnection, sql_query},
    key_package_history::StoredKeyPackageHistoryEntry,
    prelude::{QueryIdentity, QueryIdentityUpdates, QueryKeyPackageHistory},
    sql_key_store::{KEY_PACKAGE_REFERENCES, KEY_PACKAGE_WRAPPER_PRIVATE_KEY},
};
use xmtp_db::{
    EncryptedMessageStore, MlsProviderExt, StorageOption, XmtpOpenMlsProvider,
    group_message::StoredGroupMessage,
};
use xmtp_id::{
    InboxOwner,
    associations::{
        Identifier, ident,
        test_utils::MockSmartContractSignatureVerifier,
        unverified::{UnverifiedIdentityUpdate, UnverifiedPasskeySignature, UnverifiedSignature},
    },
    scw_verifier::SmartContractSignatureVerifier,
};
use xmtp_proto::{
    api::ApiClientError,
    api_client::{ApiBuilder, ToxicProxies, ToxicTestClient},
    identity_v1::PublishIdentityUpdateRequest,
    prelude::XmtpTestClient,
    xmtp::{
        device_sync::{BackupElement, backup_element::Element},
        identity::associations::IdentityUpdate,
        message_contents::PrivateKey,
    },
};

type XmtpMlsProvider = XmtpOpenMlsProvider<Arc<TestMlsStorage>>;

/// A test client wrapper that auto-exposes all of the usual component access boilerplate.
/// Makes testing easier and less repetitive.
pub struct Tester<Owner = PrivateKeySigner, Client = FullXmtpClient>
where
    Owner: InboxOwner,
{
    pub builder: TesterBuilder<Owner>,
    pub client: Client,
    pub worker: Option<Arc<WorkerMetrics<SyncMetric>>>,
    #[cfg(target_arch = "wasm32")]
    pub stream_handle: Option<Box<dyn StreamHandle<StreamOutput = Result<(), SubscribeError>>>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub stream_handle:
        Option<Box<dyn StreamHandle<StreamOutput = Result<(), SubscribeError>> + Send>>,
    pub proxy: Option<ToxicProxies>,
}

impl<Owner> Tester<Owner, FullXmtpClient>
where
    Owner: InboxOwner,
{
    pub fn db_snapshot(&self) -> Vec<u8> {
        if !self.builder.ephemeral_db {
            panic!("Snapshots can only be made on ephemeral databases.");
        }

        self.db()
            .raw_query(|conn| {
                let buff = conn.serialize_database_to_buffer();
                Ok(buff.to_vec())
            })
            .unwrap()
    }

    pub fn save_snapshot_to_file(&self, path: impl AsRef<Path>) {
        let snapshot = self.db_snapshot();
        std::fs::write(path, &snapshot).unwrap();
    }

    pub fn identifier(&self) -> Identifier {
        self.builder.owner.get_identifier().unwrap()
    }
}

#[derive(QueryableByName)]
struct TableName {
    #[diesel(sql_type = diesel::sql_types::Text)]
    name: String,
}

#[xmtp_common::async_trait]
pub trait LocalTester {
    async fn new() -> Self;
    async fn new_passkey() -> Tester<PasskeyUser, FullXmtpClient>;
    fn builder() -> TesterBuilder<PrivateKeySigner>;
}

#[xmtp_common::async_trait]
impl LocalTester for Tester<PrivateKeySigner, FullXmtpClient> {
    async fn new() -> Self {
        let wallet = generate_local_wallet();
        Tester::new_with_owner(wallet).await
    }

    async fn new_passkey() -> Tester<PasskeyUser, FullXmtpClient> {
        let passkey_user = PasskeyUser::new().await;
        Tester::new_with_owner(passkey_user).await
    }

    fn builder() -> TesterBuilder<PrivateKeySigner> {
        TesterBuilder::new()
    }
}

#[allow(async_fn_in_trait)]
pub trait LocalTesterBuilder<Owner, C>
where
    Owner: InboxOwner,
{
    async fn build(&self) -> Tester<Owner, C>;
}

impl<Owner> LocalTesterBuilder<Owner, FullXmtpClient> for TesterBuilder<Owner>
where
    Owner: InboxOwner + Clone + 'static,
{
    async fn build(&self) -> Tester<Owner, FullXmtpClient> {
        let strategy = match (&self.external_identity, &self.snapshot) {
            (Some(identity), _) => IdentityStrategy::ExternalIdentity(identity.clone()),
            (_, Some(snapshot)) => IdentityStrategy::CachedOnly,
            _ => identity_setup(&self.owner),
        };
        let mut client = Client::builder(strategy.clone());

        // Setup the database. Snapshots are always ephemeral.
        if self.ephemeral_db || self.snapshot.is_some() {
            let db = if let Some(snapshot) = &self.snapshot {
                client.allow_offline = true;
                TestDb::create_ephemeral_store_from_snapshot(snapshot, self.snapshot_path.as_ref())
                    .await
            } else {
                TestDb::create_ephemeral_store().await
            };

            client = client.store(db);
        } else {
            client = client.temp_store().await;
        }

        let mut proxy = None;
        let api_client = if self.proxy {
            proxy = Some(ToxicOnlyTestClientCreator::proxies().await);
            ToxicOnlyTestClientCreator::create().build().unwrap()
        } else {
            DefaultTestClientCreator::create().build().unwrap()
        };
        let api_client = self
            .api_client
            .clone()
            .unwrap_or_else(|| Arc::new(api_client));

        let mut client = client
            .api_client(api_client)
            .with_disable_workers(self.disable_workers)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .with_device_sync_worker_mode(Some(self.sync_mode))
            .maybe_version(self.version.clone())
            .with_commit_log_worker(self.commit_log_worker)
            .fork_recovery_opts(self.fork_recovery_opts.clone().unwrap_or_default())
            .worker_config(self.worker_config.clone().unwrap_or_default())
            .unstable_change_callbacks(self.change_callbacks.clone());

        if self.triggers {
            client = client.enable_sqlite_triggers();
        }

        let client = client.default_mls_store().unwrap().build().await.unwrap();

        if let IdentityStrategy::CreateIfNotFound { .. } = &strategy {
            register_client(&client, &self.owner).await;
        }

        let mut worker = None;
        if self.wait_for_init && self.sync_mode != DeviceSyncMode::Disabled {
            while worker.is_none() {
                xmtp_common::task::yield_now().await;
                worker = client.context.sync_metrics();
            }
            worker.as_ref().unwrap().wait_for_init().await.unwrap();
        }

        let mut tester = Tester {
            builder: self.clone(),
            client,
            worker,
            stream_handle: None,
            proxy,
        };

        // If the tester is loaded from a snapshot, we need to do some housekeeping,
        // because the client and the server are now out-of-sync.
        if self.snapshot.is_some() {
            tester.publish_all_identity_updates().await;
            tester.reset_identity_and_refresh_state();
            tester.rotate_and_upload_key_package().await.unwrap();
            load_identity_updates(
                &tester.context.api_client,
                &tester.db(),
                &[tester.inbox_id()],
            )
            .await
            .unwrap();
        }

        tester.sync_welcomes().await;
        if self.stream {
            tester.stream();
        }

        if let Some(name) = &self.name {
            tester.set_name(name);
        }

        tester
    }
}

impl<Owner> Tester<Owner, FullXmtpClient>
where
    Owner: InboxOwner + Clone + 'static,
{
    async fn publish_all_identity_updates(&self) {
        let updates = self
            .db()
            .get_identity_updates(self.inbox_id(), None, None)
            .unwrap();
        for update in updates {
            let update: UnverifiedIdentityUpdate = update.payload.try_into().unwrap();
            let result = self.context.api().publish_identity_update(update).await;

            if let Err(err) = result {
                tracing::warn!("{err:?}");
            }
        }
    }
    fn reset_identity_and_refresh_state(&self) {
        self.context
            .db()
            .raw_query(|c| {
                xmtp_db::diesel::delete(xmtp_db::schema::association_state::table)
                    .execute(c)
                    .unwrap();
                xmtp_db::diesel::delete(xmtp_db::schema::identity_cache::table)
                    .execute(c)
                    .unwrap();
                xmtp_db::diesel::delete(xmtp_db::schema::identity_updates::table)
                    .execute(c)
                    .unwrap();
                xmtp_db::diesel::delete(xmtp_db::schema::refresh_state::table)
                    .execute(c)
                    .unwrap();

                Ok(())
            })
            .unwrap();
    }

    pub async fn new_with_owner(owner: Owner) -> Self {
        TesterBuilder::new().owner(owner).build().await
    }

    fn stream(&mut self) {
        let handle = FullXmtpClient::stream_all_messages_with_callback(
            self.client.context.clone(),
            None,
            None,
            |_| {},
            || {},
        );
        let handle = Box::new(handle) as Box<_>;
        self.stream_handle = Some(handle);
    }

    fn provider(&self) -> impl MlsProviderExt + use<'_, Owner> {
        self.client.context.mls_provider()
    }
}

#[allow(dead_code)]
impl<Owner, Client> Tester<Owner, Client>
where
    Owner: InboxOwner + Clone + 'static,
{
    pub fn builder_from(owner: Owner) -> TesterBuilder<Owner> {
        TesterBuilder::new().owner(owner)
    }

    /// Create a new installations for this client
    pub async fn new_installation(&self) -> Tester<Owner, FullXmtpClient> {
        TesterBuilder::new()
            .owner(self.builder.owner.clone())
            .build()
            .await
    }
    pub fn worker(&self) -> &Arc<WorkerMetrics<SyncMetric>> {
        self.worker.as_ref().unwrap()
    }

    pub fn proxies(&self) -> &ToxicProxies {
        self.proxy.as_ref().unwrap()
    }

    pub fn proxy(&self, n: usize) -> &Proxy {
        self.proxy.as_ref().unwrap().proxy(n)
    }

    pub async fn for_each_proxy<F>(&self, f: F)
    where
        F: AsyncFn(&Proxy),
    {
        self.proxy.as_ref().unwrap().for_each(f).await
    }
}

impl<Owner, Client> Deref for Tester<Owner, Client>
where
    Owner: InboxOwner,
{
    type Target = Client;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

#[derive(Clone)]
pub struct TesterBuilder<Owner>
where
    Owner: InboxOwner,
{
    pub owner: Owner,
    pub sync_mode: DeviceSyncMode,
    pub fork_recovery_opts: Option<ForkRecoveryOpts>,
    pub wait_for_init: bool,
    pub stream: bool,
    pub name: Option<String>,
    pub version: Option<VersionInfo>,
    pub proxy: bool,
    pub api_client: Option<crate::utils::TestClient>,
    pub commit_log_worker: bool,
    pub ephemeral_db: bool,
    pub api_endpoint: ApiEndpoint,
    pub triggers: bool,
    pub external_identity: Option<Identity>,
    pub snapshot: Option<Arc<Vec<u8>>>,
    pub snapshot_path: Option<PathBuf>,
    /// whether this builder represents a second installation
    pub installation: bool,
    pub disable_workers: bool,
    pub worker_config: Option<crate::worker::WorkerConfig>,
    pub change_callbacks: crate::groups::change_callbacks::UnstableChangeCallbacks,
}

#[derive(Clone)]
pub enum ApiEndpoint {
    Local,
    Dev,
}

impl TesterBuilder<PrivateKeySigner> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for TesterBuilder<PrivateKeySigner> {
    fn default() -> Self {
        Self {
            owner: generate_local_wallet(),
            sync_mode: DeviceSyncMode::Disabled,
            fork_recovery_opts: None,
            wait_for_init: true,
            stream: false,
            name: None,
            version: None,
            proxy: false,
            api_client: None,
            commit_log_worker: true, // Default to enabled to match production
            installation: false,
            ephemeral_db: true,
            triggers: false,
            api_endpoint: ApiEndpoint::Local,
            external_identity: None,
            snapshot: None,
            snapshot_path: None,
            disable_workers: false,
            worker_config: None,
            change_callbacks: Default::default(),
        }
    }
}

impl<Owner> TesterBuilder<Owner>
where
    Owner: InboxOwner,
{
    pub fn owner<NewOwner>(self, owner: NewOwner) -> TesterBuilder<NewOwner>
    where
        NewOwner: InboxOwner,
    {
        TesterBuilder {
            owner,
            sync_mode: self.sync_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            wait_for_init: self.wait_for_init,
            stream: self.stream,
            name: self.name,
            version: self.version,
            proxy: self.proxy,
            api_client: self.api_client,
            commit_log_worker: self.commit_log_worker,
            installation: self.installation,
            ephemeral_db: self.ephemeral_db,
            api_endpoint: self.api_endpoint,
            triggers: self.triggers,
            external_identity: self.external_identity,
            snapshot: self.snapshot,
            snapshot_path: self.snapshot_path,
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
            change_callbacks: self.change_callbacks,
        }
    }

    pub fn api_client(mut self, api_client: crate::utils::TestClient) -> Self {
        self.api_client = Some(api_client);
        self
    }

    /// Assign a name to this tester
    /// Replaces log output of InstallationIds, Identifiers, and InboxIds
    /// when using CONTEXTUAL = 1
    pub fn with_name(self, s: &str) -> TesterBuilder<Owner> {
        Self {
            name: Some(s.to_string()),
            ..self
        }
    }

    pub fn version(self, version: VersionInfo) -> Self {
        Self {
            version: Some(version),
            ..self
        }
    }

    pub fn passkey(self) -> TesterBuilder<PasskeyUser> {
        self.owner(block_on(async { PasskeyUser::new().await }))
    }

    pub fn dev(mut self) -> Self {
        self.api_endpoint = ApiEndpoint::Dev;
        self
    }

    pub fn with_dev(mut self, dev: bool) -> Self {
        self.api_endpoint = match dev {
            true => ApiEndpoint::Dev,
            false => ApiEndpoint::Local,
        };
        self
    }

    pub fn external_identity(mut self, identity: Identity) -> Self {
        self.external_identity = Some(identity);
        self
    }

    pub fn with_external_identity(mut self, identity: Option<Identity>) -> Self {
        self.external_identity = identity;
        self
    }

    pub fn snapshot(mut self, snapshot: Arc<Vec<u8>>) -> Self {
        self.snapshot = Some(snapshot);
        self.ephemeral_db = true;
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn snapshot_file(mut self, snapshot_path: impl Into<PathBuf>) -> Self {
        let snapshot_path = snapshot_path.into();
        let snapshot = std::fs::read(&snapshot_path).unwrap();
        self.snapshot_path = Some(snapshot_path);
        self.snapshot(Arc::new(snapshot))
    }

    pub fn disable_workers(mut self) -> Self {
        self.disable_workers = true;
        self
    }

    pub fn worker_config(mut self, cfg: crate::worker::WorkerConfig) -> Self {
        self.worker_config = Some(cfg);
        self
    }

    pub fn change_callbacks(
        mut self,
        callbacks: crate::groups::change_callbacks::UnstableChangeCallbacks,
    ) -> Self {
        self.change_callbacks = callbacks;
        self
    }

    pub fn with_snapshot(mut self, snapshot: Option<Arc<Vec<u8>>>) -> Self {
        if let Some(snapshot) = snapshot {
            self = self.snapshot(snapshot);
        }
        self
    }

    pub fn triggers(mut self) -> Self {
        self.triggers = true;
        self
    }

    pub fn enable_fork_recovery_requests(self) -> Self {
        Self {
            fork_recovery_opts: Some(ForkRecoveryOpts {
                enable_recovery_requests: ForkRecoveryPolicy::All,
                groups_to_request_recovery: vec![],
                disable_recovery_responses: false,
                worker_interval_ns: None,
            }),
            ..self
        }
    }

    pub fn enable_fork_recovery_requests_for(self, groups: Vec<String>) -> Self {
        Self {
            fork_recovery_opts: Some(ForkRecoveryOpts {
                enable_recovery_requests: ForkRecoveryPolicy::AllowlistedGroups,
                groups_to_request_recovery: groups,
                disable_recovery_responses: false,
                worker_interval_ns: None,
            }),
            ..self
        }
    }

    pub fn disable_fork_recovery_responses(self) -> Self {
        Self {
            fork_recovery_opts: Some(ForkRecoveryOpts {
                enable_recovery_requests: ForkRecoveryPolicy::None,
                groups_to_request_recovery: vec![],
                disable_recovery_responses: true,
                worker_interval_ns: None,
            }),
            ..self
        }
    }

    pub fn stream(self) -> Self {
        Self {
            stream: true,
            ..self
        }
    }

    pub fn sync_worker(mut self) -> Self {
        self.sync_mode = DeviceSyncMode::Enabled;
        self
    }

    pub fn sync_mode(self, sync_mode: DeviceSyncMode) -> Self {
        Self { sync_mode, ..self }
    }

    pub fn persistent_db(mut self) -> Self {
        self.ephemeral_db = false;
        self
    }

    pub fn proxy(mut self) -> Self {
        self.proxy = true;
        self
    }

    pub fn with_commit_log_worker(mut self, enabled: bool) -> Self {
        self.commit_log_worker = enabled;
        self
    }

    pub fn installation(mut self) -> Self {
        self.installation = true;
        self
    }

    pub fn do_not_wait_for_init(mut self) -> Self {
        self.wait_for_init = false;
        self
    }
}

#[macro_export]
macro_rules! tester {
    ($name:ident, from: $existing:expr $(, $k:ident $(: $v:expr)?)*) => {
        $crate::tester!(@process $existing.builder.clone() ; $name $(, $k $(: $v)?)*)
    };

    ($name:ident $(, $k:ident $(: $v:expr)?)*) => {
        let builder = $crate::utils::TesterBuilder::new();
        let builder = builder.with_name(stringify!($name));
        $crate::tester!(@process builder ; $name $(, $k $(: $v)?)*)
    };

    (@process $builder:expr ; $name:ident) => {
        let $name = {
            use tracing::Instrument;

            use $crate::utils::LocalTesterBuilder;
            let span = tracing::info_span!(stringify!($name));
            $builder.build().instrument(span).await
        };
    };

    (@process $builder:expr ; $name:ident, $key:ident: $value:expr $(, $k:ident $(: $v:expr)?)*) => {
        $crate::tester!(@process $builder.$key($value) ; $name $(, $k $(: $v)?)*)
    };

    (@process $builder:expr ; $name:ident, $key:ident $(, $k:ident $(: $v:expr)?)*) => {
        $crate::tester!(@process $builder.$key() ; $name $(, $k $(: $v)?)*)
    };
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_snapshots() {
        tester!(alix);
        let g = alix.create_group(None, None)?;
        let snap = Arc::new(alix.db_snapshot());
        tester!(alix2, snapshot: snap);

        assert_eq!(alix.inbox_id(), alix2.inbox_id());
        assert!(alix2.group(&g.group_id).is_ok());
    }
}
