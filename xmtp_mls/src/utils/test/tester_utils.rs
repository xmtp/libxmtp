#![allow(unused)]

use super::FullXmtpClient;
use xmtp_configuration::DeviceSyncUrls;

use crate::{
    Client,
    builder::{ClientBuilder, SyncWorkerMode},
    client::ClientError,
    context::XmtpSharedContext,
    groups::device_sync::worker::SyncMetric,
    subscriptions::SubscribeError,
    utils::{TestClient, TestMlsStorage, VersionInfo, register_client},
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
use xmtp_api_http::{LOCALHOST_ADDRESS, constants::ApiUrls};
use xmtp_common::StreamHandle;
use xmtp_common::TestLogReplace;
use xmtp_cryptography::{signature::SignatureError, utils::generate_local_wallet};
use xmtp_db::{
    MlsProviderExt, XmtpOpenMlsProvider, group_message::StoredGroupMessage,
    sql_key_store::SqlKeyStore,
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
use xmtp_proto::prelude::XmtpTestClient;

pub static TOXIPROXY: OnceCell<toxiproxy_rust::client::Client> = OnceCell::const_new();
pub static TOXI_PORT: AtomicUsize = AtomicUsize::new(21100);
type XmtpMlsProvider = XmtpOpenMlsProvider<Arc<TestMlsStorage>>;

/// A test client wrapper that auto-exposes all of the usual component access boilerplate.
/// Makes testing easier and less repetetive.
pub struct Tester<Owner, Client>
where
    Owner: InboxOwner,
{
    pub builder: TesterBuilder<Owner>,
    pub client: Client,
    pub worker: Option<Arc<WorkerMetrics<SyncMetric>>>,
    pub stream_handle: Option<Box<dyn StreamHandle<StreamOutput = Result<(), SubscribeError>>>>,
    pub proxy: Option<Proxy>,
    /// Replacement names for this tester
    /// Replacements are removed on drop
    pub replace: TestLogReplace,
}

#[macro_export]
macro_rules! tester {
    ($name:ident, from: $existing:expr $(, $k:ident $(: $v:expr)?)*) => {
        tester!(@process $existing.builder ; $name $(, $k $(: $v)?)*)
    };

    ($name:ident $(, $k:ident $(: $v:expr)?)*) => {
        let builder = $crate::utils::Tester::builder();
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

impl Tester<PrivateKeySigner, FullXmtpClient> {
    pub(crate) async fn new() -> Tester<PrivateKeySigner, FullXmtpClient> {
        let wallet = generate_local_wallet();
        Tester::new_with_owner(wallet).await
    }

    pub(crate) async fn new_passkey() -> Tester<PasskeyUser, FullXmtpClient> {
        let passkey_user = PasskeyUser::new().await;
        Tester::new_with_owner(passkey_user).await
    }

    pub(crate) fn builder() -> TesterBuilder<PrivateKeySigner> {
        TesterBuilder::new()
    }
}

pub(crate) trait LocalTesterBuilder<Owner, C>
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
        if let Some(name) = &self.name {
            let ident = self.owner.get_identifier().unwrap();
            replace.add(&ident.to_string(), &format!("{name}_ident"));
        }
        let mut api_addr = format!("localhost:{}", ClientBuilder::local_port());
        let mut proxy = None;

        if self.proxy {
            let toxiproxy = TOXIPROXY
                .get_or_init(|| async {
                    let toxiproxy = toxiproxy_rust::client::Client::new("0.0.0.0:8474");
                    toxiproxy.reset().await.unwrap();
                    toxiproxy
                })
                .await;

            let port = TOXI_PORT.fetch_add(1, Ordering::SeqCst);

            let result = toxiproxy
                .populate(vec![
                    ProxyPack::new(
                        format!("Proxy {port}"),
                        format!("[::]:{port}"),
                        format!("node:{}", ClientBuilder::local_port()),
                    )
                    .await,
                ])
                .await
                .unwrap();

            proxy = Some(result.into_iter().nth(0).unwrap());
            api_addr = format!("localhost:{port}");
        }

        let api_client = ClientBuilder::new_custom_api_client(&format!("http://{api_addr}")).await;
        let sync_api_client =
            ClientBuilder::new_custom_api_client(&format!("http://{api_addr}")).await;
        let client = ClientBuilder::new_test_builder(&self.owner)
            .await
            .api_clients(api_client, sync_api_client)
            .with_device_sync_worker_mode(Some(self.sync_mode))
            .with_device_sync_server_url(self.sync_url.clone())
            .maybe_version(self.version.clone())
            .with_disable_events(Some(!self.events))
            .build()
            .await
            .unwrap();
        register_client(&client, &self.owner).await;
        if let Some(name) = &self.name {
            replace.add(
                &client.installation_public_key().to_string(),
                &format!("{name}_installation"),
            );
            replace.add(client.inbox_id(), name);
        }
        let worker = client.context.sync_metrics();
        if let Some(worker) = &worker {
            if self.wait_for_init {
                worker.wait_for_init().await.unwrap();
            }
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
    pub(crate) async fn new_with_owner(owner: Owner) -> Self {
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
    pub fn worker(&self) -> &Arc<WorkerMetrics<SyncMetric>> {
        self.worker.as_ref().unwrap()
    }

    pub fn proxy(&self) -> &Proxy {
        self.proxy.as_ref().unwrap()
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
    pub wait_for_init: bool,
    pub stream: bool,
    pub name: Option<String>,
    pub events: bool,
    pub version: Option<VersionInfo>,
    pub proxy: bool,
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
            wait_for_init: true,
            stream: false,
            name: None,
            events: false,
            version: None,
            proxy: false,
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
            wait_for_init: self.wait_for_init,
            stream: self.stream,
            name: self.name,
            events: self.events,
            version: self.version,
            proxy: self.proxy,
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

    pub fn sync_worker(self) -> Self {
        Self {
            sync_mode: SyncWorkerMode::Enabled,
            ..self
        }
    }

    pub fn sync_server(self) -> Self {
        Self {
            sync_url: Some(DeviceSyncUrls::LOCAL_ADDRESS.to_string()),
            ..self
        }
    }

    pub fn stream(self) -> Self {
        Self {
            stream: true,
            ..self
        }
    }

    pub fn proxy(self) -> Self {
        Self {
            proxy: true,
            ..self
        }
    }

    pub fn events(self) -> Self {
        Self {
            events: true,
            ..self
        }
    }

    pub fn do_not_wait_for_init(self) -> Self {
        Self {
            wait_for_init: false,
            ..self
        }
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
