#![allow(unused)]

use super::FullXmtpClient;
use async_trait::async_trait;
use diesel::QueryableByName;
use xmtp_api_d14n::protocol::InMemoryCursorStore;
use xmtp_configuration::{DeviceSyncUrls, DockerUrls};
use xmtp_db::{
    ConnectionExt, ReadOnly, TestDb, XmtpTestDb,
    diesel::{self, Connection, RunQueryDsl, SqliteConnection, sql_query},
};

use crate::{
    Client,
    builder::{ClientBuilder, ForkRecoveryOpts, ForkRecoveryPolicy, SyncWorkerMode},
    client::ClientError,
    context::XmtpSharedContext,
    groups::device_sync::worker::SyncMetric,
    identity::{Identity, IdentityStrategy},
    subscriptions::SubscribeError,
    utils::{TestClient, TestMlsStorage, VersionInfo, register_client, test::identity_setup},
    worker::metrics::WorkerMetrics,
};
use alloy::signers::local::PrivateKeySigner;
use futures::Stream;
use futures_executor::block_on;
use parking_lot::Mutex;
use passkey::{
    authenticator::{Authenticator, UserCheck, UserValidationMethod},
    client::{Client as PasskeyClient, DefaultClientData},
    types::{Bytes, Passkey, ctap2::*, rand::random_vec, webauthn::*},
};
use public_suffix::PublicSuffixList;
use std::{
    ops::Deref,
    sync::{
        Arc, LazyLock,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::{runtime::Handle, sync::OnceCell};
use toxiproxy_rust::proxy::{Proxy, ProxyPack};
use url::Url;
use xmtp_api::XmtpApi;
use xmtp_common::StreamHandle;
use xmtp_common::TestLogReplace;
use xmtp_configuration::LOCALHOST;
use xmtp_cryptography::{signature::SignatureError, utils::generate_local_wallet};
#[cfg(not(target_arch = "wasm32"))]
use xmtp_db::NativeDb;
#[cfg(target_arch = "wasm32")]
use xmtp_db::WasmDb;
use xmtp_db::{
    EncryptedMessageStore, MlsProviderExt, StorageOption, XmtpOpenMlsProvider,
    group_message::StoredGroupMessage, sql_key_store::SqlKeyStore,
};
use xmtp_id::{
    InboxOwner,
    associations::{
        Identifier, ident,
        test_utils::MockSmartContractSignatureVerifier,
        unverified::{UnverifiedPasskeySignature, UnverifiedSignature},
    },
    scw_verifier::SmartContractSignatureVerifier,
};
use xmtp_proto::ToxicProxies;
use xmtp_proto::prelude::XmtpTestClient;
use xmtp_proto::{api_client::ApiBuilder, xmtp::message_contents::PrivateKey};

type XmtpMlsProvider = XmtpOpenMlsProvider<Arc<TestMlsStorage>>;

/// A test client wrapper that auto-exposes all of the usual component access boilerplate.
/// Makes testing easier and less repetetive.
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
    /// Replacement names for this tester
    /// Replacements are removed on drop
    pub replace: TestLogReplace,
}

impl<Owner> Tester<Owner, FullXmtpClient>
where
    Owner: InboxOwner,
{
    pub fn dump_db(&self) -> Vec<u8> {
        self.db()
            .raw_query_write(|conn| {
                let buff = conn.serialize_database_to_buffer();
                Ok(buff.to_vec())
            })
            .unwrap()
    }
}

#[derive(QueryableByName)]
struct TableName {
    #[diesel(sql_type = diesel::sql_types::Text)]
    name: String,
}

#[macro_export]
macro_rules! tester {
    ($name:ident, from: $existing:expr $(, $k:ident $(: $v:expr)?)*) => {
        tester!(@process $existing.builder ; $name $(, $k $(: $v)?)*)
    };

    ($name:ident $(, $k:ident $(: $v:expr)?)*) => {
        let builder = $crate::utils::TesterBuilder::new();
        tester!(@process builder ; $name $(, $k $(: $v)?)*)
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
        tester!(@process $builder.$key($value) ; $name $(, $k $(: $v)?)*)
    };

    (@process $builder:expr ; $name:ident, $key:ident $(, $k:ident $(: $v:expr)?)*) => {
        tester!(@process $builder.$key() ; $name $(, $k $(: $v)?)*)
    };
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait LocalTester {
    async fn new() -> Self;
    async fn new_passkey() -> Tester<PasskeyUser, FullXmtpClient>;
    fn builder() -> TesterBuilder<PrivateKeySigner>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
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
        let mut replace = TestLogReplace::default();
        if let Some(name) = &self.name
            && !self.installation
        {
            let ident = self.owner.get_identifier().unwrap();
            replace.add(&ident.to_string(), &format!("{name}_ident"));
        }

        let (mut local_client, mut sync_api_client) = match self.api_endpoint {
            ApiEndpoint::Local => (TestClient::create_local(), TestClient::create_local()),
            ApiEndpoint::Dev => (TestClient::create_dev(), TestClient::create_dev()),
        };

        let mut proxy = None;
        if self.proxy {
            proxy = Some(local_client.with_toxiproxy().await);
            sync_api_client.with_existing_toxi(local_client.host().unwrap());
        }

        let api_client = local_client.build().unwrap();
        let sync_api_client = sync_api_client.build().unwrap();

        let strategy = match (&self.external_identity, &self.snapshot) {
            (Some(identity), _) => IdentityStrategy::ExternalIdentity(identity.clone()),
            (_, Some(snapshot)) => IdentityStrategy::CachedOnly,
            _ => identity_setup(&self.owner),
        };

        let mut client = Client::builder(strategy.clone())
            .api_clients(api_client, sync_api_client)
            .with_disable_events(Some(!self.events))
            .with_disable_workers(self.disable_workers)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .with_device_sync_worker_mode(Some(self.sync_mode))
            .with_device_sync_server_url(self.sync_url.clone())
            .maybe_version(self.version.clone())
            .with_commit_log_worker(self.commit_log_worker)
            .fork_recovery_opts(self.fork_recovery_opts.clone().unwrap_or_default());

        // Setup the database. Snapshots are always ephemeral.
        if self.ephemeral_db || self.snapshot.is_some() {
            let db = TestDb::create_ephemeral_store().await;

            if let Some(snapshot) = &self.snapshot {
                db.conn()
                    .raw_query_write(|conn| conn.deserialize_database_from_buffer(snapshot))
                    .unwrap();

                client.allow_offline = true;
            }

            client = client.store(db);
        } else {
            client = client.temp_store().await;
        }

        if self.in_memory_cursors {
            client = client.cursor_store(Arc::new(InMemoryCursorStore::new()) as Arc<_>);
        }

        if self.triggers {
            client = client.enable_sqlite_triggers();
        }

        let client = client.default_mls_store().unwrap().build().await.unwrap();

        if let IdentityStrategy::CreateIfNotFound { .. } = &strategy {
            register_client(&client, &self.owner).await;
        }

        if let Some(name) = &self.name {
            replace.add(
                &client.installation_public_key().to_string(),
                &format!("{name}_installation"),
            );
            replace.add(client.inbox_id(), name);
        }
        let worker = client.context.sync_metrics();
        if let Some(worker) = &worker
            && self.wait_for_init
        {
            worker.wait_for_init().await.unwrap();
        }
        client.sync_welcomes().await;

        let mut tester = Tester {
            builder: self.clone(),
            client,
            worker,
            replace,
            stream_handle: None,
            proxy,
        };

        if self.stream {
            tester.stream();
        }

        tester
    }
}

impl<Owner> Tester<Owner, FullXmtpClient>
where
    Owner: InboxOwner + Clone + 'static,
{
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
    pub sync_mode: SyncWorkerMode,
    pub sync_url: Option<String>,
    pub fork_recovery_opts: Option<ForkRecoveryOpts>,
    pub wait_for_init: bool,
    pub stream: bool,
    pub name: Option<String>,
    pub events: bool,
    pub version: Option<VersionInfo>,
    pub proxy: bool,
    pub commit_log_worker: bool,
    pub in_memory_cursors: bool,
    pub ephemeral_db: bool,
    pub api_endpoint: ApiEndpoint,
    pub triggers: bool,
    pub external_identity: Option<Identity>,
    pub snapshot: Option<Arc<Vec<u8>>>,
    /// whether this builder represents a second installation
    pub installation: bool,
    pub disable_workers: bool,
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
            sync_mode: SyncWorkerMode::Disabled,
            sync_url: None,
            fork_recovery_opts: None,
            wait_for_init: true,
            stream: false,
            name: None,
            events: false,
            version: None,
            proxy: false,
            commit_log_worker: true, // Default to enabled to match production
            installation: false,
            in_memory_cursors: false,
            ephemeral_db: false,
            triggers: false,
            api_endpoint: ApiEndpoint::Local,
            external_identity: None,
            snapshot: None,
            disable_workers: false,
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
            sync_url: self.sync_url,
            fork_recovery_opts: self.fork_recovery_opts,
            wait_for_init: self.wait_for_init,
            stream: self.stream,
            name: self.name,
            events: self.events,
            version: self.version,
            proxy: self.proxy,
            commit_log_worker: self.commit_log_worker,
            installation: self.installation,
            in_memory_cursors: self.in_memory_cursors,
            ephemeral_db: self.ephemeral_db,
            api_endpoint: self.api_endpoint,
            triggers: self.triggers,
            external_identity: self.external_identity,
            snapshot: self.snapshot,
            disable_workers: self.disable_workers,
        }
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
        self.ephemeral_db()
    }

    pub fn disable_workers(mut self) -> Self {
        self.disable_workers = true;
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
        self.sync_mode = SyncWorkerMode::Enabled;
        self
    }

    pub fn sync_server(mut self) -> Self {
        self.sync_url = Some(DeviceSyncUrls::LOCAL_ADDRESS.to_string());
        self
    }

    pub fn ephemeral_db(mut self) -> Self {
        self.ephemeral_db = true;
        self
    }

    pub fn in_memory_cursors(mut self) -> Self {
        self.in_memory_cursors = true;
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

    pub fn events(mut self) -> Self {
        self.events = true;
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

    pub fn sync_mode(self, sync_mode: SyncWorkerMode) -> Self {
        Self { sync_mode, ..self }
    }
}

pub type PKCredential = PublicKeyCredential<AuthenticatorAttestationResponse>;
pub type PKClient = PasskeyClient<Option<Passkey>, PkUserValidationMethod, PublicSuffixList>;

#[derive(Clone)]
pub struct PasskeyUser {
    origin: Url,
    pk_cred: Arc<PKCredential>,
    pk_client: Arc<Mutex<PKClient>>,
}

impl InboxOwner for PasskeyUser {
    fn sign(&self, text: &str) -> Result<UnverifiedSignature, SignatureError> {
        let text = text.as_bytes().to_vec();
        let sign_request = CredentialRequestOptions {
            public_key: PublicKeyCredentialRequestOptions {
                challenge: Bytes::from(text),
                timeout: None,
                rp_id: Some(String::from(self.origin.domain().unwrap())),
                allow_credentials: None,
                user_verification: UserVerificationRequirement::default(),
                hints: None,
                attestation: AttestationConveyancePreference::None,
                attestation_formats: None,
                extensions: None,
            },
        };

        let mut pk_client = self.pk_client.lock();

        let cred = pk_client.authenticate(self.origin.clone(), sign_request, DefaultClientData);
        let cred = futures_executor::block_on(cred).unwrap();
        let resp = cred.response;

        let signature = resp.signature.to_vec();

        Ok(UnverifiedSignature::Passkey(UnverifiedPasskeySignature {
            public_key: self.public_key(),
            signature,
            authenticator_data: resp.authenticator_data.to_vec(),
            client_data_json: resp.client_data_json.to_vec(),
        }))
    }

    fn get_identifier(
        &self,
    ) -> Result<
        xmtp_id::associations::Identifier,
        xmtp_cryptography::signature::IdentifierValidationError,
    > {
        Ok(Identifier::Passkey(ident::Passkey {
            key: self.public_key(),
            relying_party: None,
        }))
    }
}

impl PasskeyUser {
    pub async fn new() -> Self {
        let origin = url::Url::parse("https://xmtp.chat").expect("Should parse");
        let parameters_from_rp = PublicKeyCredentialParameters {
            ty: PublicKeyCredentialType::PublicKey,
            alg: coset::iana::Algorithm::ES256,
        };
        let pk_user_entity = PublicKeyCredentialUserEntity {
            id: random_vec(32).into(),
            display_name: "Alex Passkey".into(),
            name: "apk@example.org".into(),
        };
        let pk_auth_store: Option<Passkey> = None;
        let pk_aaguid = Aaguid::new_empty();
        let pk_user_validation_method = PkUserValidationMethod {};
        let pk_auth = Authenticator::new(pk_aaguid, pk_auth_store, pk_user_validation_method);
        let mut pk_client = PasskeyClient::new(pk_auth);

        let request = CredentialCreationOptions {
            public_key: PublicKeyCredentialCreationOptions {
                rp: PublicKeyCredentialRpEntity {
                    id: None, // Leaving the ID as None means use the effective domain
                    name: origin.domain().unwrap().into(),
                },
                user: pk_user_entity,
                // We're not passing a challenge here because we don't care about the credential and the user_entity behind it (for now).
                // It's guaranteed to be unique, and that's good enough for us.
                // All we care about is if that unique credential signs below.
                challenge: Bytes::from(vec![]),
                pub_key_cred_params: vec![parameters_from_rp],
                timeout: None,
                exclude_credentials: None,
                authenticator_selection: None,
                hints: None,
                attestation: AttestationConveyancePreference::None,
                attestation_formats: None,
                extensions: None,
            },
        };

        // Now create the credential.
        let pk_cred = pk_client
            .register(origin.clone(), request, DefaultClientData)
            .await
            .unwrap();

        Self {
            pk_client: Arc::new(Mutex::new(pk_client)),
            pk_cred: Arc::new(pk_cred),
            origin,
        }
    }

    fn public_key(&self) -> Vec<u8> {
        self.pk_cred.response.public_key.as_ref().unwrap()[26..].to_vec()
    }

    pub fn identifier(&self) -> Identifier {
        Identifier::Passkey(ident::Passkey {
            key: self.public_key(),
            relying_party: self.origin.domain().map(str::to_string),
        })
    }
}

pub struct PkUserValidationMethod {}
#[async_trait::async_trait]
impl UserValidationMethod for PkUserValidationMethod {
    type PasskeyItem = Passkey;
    async fn check_user<'a>(
        &self,
        _credential: Option<&'a Passkey>,
        presence: bool,
        verification: bool,
    ) -> Result<UserCheck, Ctap2Error> {
        Ok(UserCheck {
            presence,
            verification,
        })
    }

    fn is_verification_enabled(&self) -> Option<bool> {
        Some(true)
    }

    fn is_presence_enabled(&self) -> bool {
        true
    }
}
