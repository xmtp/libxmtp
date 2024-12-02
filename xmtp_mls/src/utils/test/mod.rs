#![allow(clippy::unwrap_used)]

use rand::{
    distributions::{Alphanumeric, DistString},
    Rng, RngCore,
};
use std::sync::Arc;
use tokio::{sync::Notify, time::error::Elapsed};
use xmtp_id::{
    associations::{
        generate_inbox_id,
        test_utils::MockSmartContractSignatureVerifier,
        unverified::{UnverifiedRecoverableEcdsaSignature, UnverifiedSignature},
    },
    scw_verifier::SmartContractSignatureVerifier,
};
use xmtp_proto::api_client::XmtpTestClient;

use crate::{
    builder::ClientBuilder,
    identity::IdentityStrategy,
    storage::{EncryptedMessageStore, StorageOption},
    types::Address,
    Client, InboxOwner, XmtpApi,
};

#[cfg(not(target_arch = "wasm32"))]
pub mod traced_test;
#[cfg(not(target_arch = "wasm32"))]
pub use traced_test::traced_test;

pub type FullXmtpClient = Client<TestClient, MockSmartContractSignatureVerifier>;

// TODO: Dev-Versions of URL
const HISTORY_SERVER_HOST: &str = "localhost";
const HISTORY_SERVER_PORT: u16 = 5558;
pub const HISTORY_SYNC_URL: &str =
    const_format::concatcp!("http://", HISTORY_SERVER_HOST, ":", HISTORY_SERVER_PORT);

#[cfg(not(any(feature = "http-api", target_arch = "wasm32")))]
pub type TestClient = xmtp_api_grpc::grpc_api_helper::Client;

#[cfg(any(feature = "http-api", target_arch = "wasm32"))]
use xmtp_api_http::XmtpHttpApiClient;
#[cfg(any(feature = "http-api", target_arch = "wasm32"))]
pub type TestClient = XmtpHttpApiClient;

pub fn rand_string() -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), 24)
}

pub fn rand_account_address() -> Address {
    Alphanumeric.sample_string(&mut rand::thread_rng(), 42)
}

pub fn rand_vec() -> Vec<u8> {
    rand::thread_rng().gen::<[u8; 24]>().to_vec()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn tmp_path() -> String {
    let db_name = rand_string();
    format!("{}/{}.db3", std::env::temp_dir().to_str().unwrap(), db_name)
}

#[cfg(target_arch = "wasm32")]
pub fn tmp_path() -> String {
    let db_name = rand_string();
    format!("{}/{}.db3", "test_db", db_name)
}

pub fn rand_time() -> i64 {
    let mut rng = rand::thread_rng();
    rng.gen_range(0..1_000_000_000)
}

impl EncryptedMessageStore {
    pub fn generate_enc_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        xmtp_cryptography::utils::rng().fill_bytes(&mut key[..]);
        key
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn remove_db_files<P: AsRef<str>>(path: P) {
        use crate::storage::EncryptedConnection;

        let path = path.as_ref();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(EncryptedConnection::salt_file(path).unwrap()).unwrap();
    }

    /// just a no-op on wasm32
    #[cfg(target_arch = "wasm32")]
    pub fn remove_db_files<P: AsRef<str>>(_path: P) {}
}

impl<A, V> ClientBuilder<A, V> {
    pub async fn temp_store(self) -> Self {
        let tmpdb = tmp_path();
        tracing::info!("Opening Database at [{}]", tmpdb);
        self.store(
            EncryptedMessageStore::new(
                StorageOption::Persistent(tmpdb),
                EncryptedMessageStore::generate_enc_key(),
            )
            .await
            .unwrap(),
        )
    }
}
impl ClientBuilder<TestClient, MockSmartContractSignatureVerifier> {
    pub async fn new_test_client(owner: &impl InboxOwner) -> FullXmtpClient {
        let api_client = <TestClient as XmtpTestClient>::create_local().await;

        build_with_verifier(
            owner,
            api_client,
            MockSmartContractSignatureVerifier::new(true),
            None,
        )
        .await
    }

    pub async fn new_test_client_with_history(
        owner: &impl InboxOwner,
        history_sync_url: &str,
    ) -> FullXmtpClient {
        let api_client = <TestClient as XmtpTestClient>::create_local().await;

        build_with_verifier(
            owner,
            api_client,
            MockSmartContractSignatureVerifier::new(true),
            Some(history_sync_url),
        )
        .await
    }

    /// A client pointed at the dev network with a Mock verifier (never fail to verify)
    pub async fn new_mock_dev_client(
        owner: impl InboxOwner,
    ) -> Client<TestClient, MockSmartContractSignatureVerifier> {
        let api_client = <TestClient as XmtpTestClient>::create_dev().await;

        build_with_verifier(
            owner,
            api_client,
            MockSmartContractSignatureVerifier::new(true),
            None,
        )
        .await
    }
}

impl ClientBuilder<TestClient> {
    /// Create a client pointed at the local container with the default remote verifier
    pub async fn new_local_client(owner: &impl InboxOwner) -> Client<TestClient> {
        let api_client = <TestClient as XmtpTestClient>::create_local().await;
        inner_build(owner, api_client).await
    }

    pub async fn new_dev_client(owner: &impl InboxOwner) -> Client<TestClient> {
        let api_client = <TestClient as XmtpTestClient>::create_dev().await;
        inner_build(owner, api_client).await
    }

    /// Add the local client to this builder
    pub async fn local_client(self) -> Self {
        self.api_client(<TestClient as XmtpTestClient>::create_local().await)
    }

    pub async fn dev_client(self) -> Self {
        self.api_client(<TestClient as XmtpTestClient>::create_dev().await)
    }
}

async fn inner_build<A>(owner: impl InboxOwner, api_client: A) -> Client<A>
where
    A: XmtpApi + 'static,
{
    let nonce = 1;
    let inbox_id = generate_inbox_id(&owner.get_address(), &nonce).unwrap();

    let client = Client::<A>::builder(IdentityStrategy::new(
        inbox_id,
        owner.get_address(),
        nonce,
        None,
    ));

    let client = client
        .temp_store()
        .await
        .api_client(api_client)
        .build()
        .await
        .unwrap();

    register_client(&client, owner).await;

    client
}

async fn build_with_verifier<A, V>(
    owner: impl InboxOwner,
    api_client: A,
    scw_verifier: V,
    history_sync_url: Option<&str>,
) -> Client<A, V>
where
    A: XmtpApi + Send + Sync + 'static,
    V: SmartContractSignatureVerifier + Send + Sync + 'static,
{
    let nonce = 1;
    let inbox_id = generate_inbox_id(&owner.get_address(), &nonce).unwrap();

    let mut builder = Client::<A, V>::builder(IdentityStrategy::new(
        inbox_id,
        owner.get_address(),
        nonce,
        None,
    ))
    .temp_store()
    .await
    .api_client(api_client)
    .scw_signature_verifier(scw_verifier);

    if let Some(history_sync_url) = history_sync_url {
        builder = builder.history_sync_url(history_sync_url);
    }

    let client = builder.build_with_verifier().await.unwrap();

    register_client(&client, owner).await;

    if client.history_sync_url.is_some() {
        let provider = client.mls_provider().unwrap();
        client.start_sync_worker(&provider).await.unwrap();
    }

    client
}

/// wrapper over a `Notify` with a 60-scond timeout for waiting
#[derive(Clone, Default)]
pub struct Delivery {
    notify: Arc<Notify>,
    timeout: core::time::Duration,
}

impl Delivery {
    pub fn new(timeout: Option<u64>) -> Self {
        let timeout = core::time::Duration::from_secs(timeout.unwrap_or(60));
        Self {
            notify: Arc::new(Notify::new()),
            timeout,
        }
    }

    pub async fn wait_for_delivery(&self) -> Result<(), Elapsed> {
        tokio::time::timeout(self.timeout, async { self.notify.notified().await }).await
    }

    pub fn notify_one(&self) {
        self.notify.notify_one()
    }
}

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi,
    V: SmartContractSignatureVerifier,
{
    pub async fn is_registered(&self, address: &String) -> bool {
        let ids = self
            .api_client
            .get_inbox_ids(vec![address.clone()])
            .await
            .unwrap();
        ids.contains_key(address)
    }
}

pub async fn register_client<T: XmtpApi, V: SmartContractSignatureVerifier>(
    client: &Client<T, V>,
    owner: impl InboxOwner,
) {
    let mut signature_request = client.context.signature_request().unwrap();
    let signature_text = signature_request.signature_text();
    let unverified_signature = UnverifiedSignature::RecoverableEcdsa(
        UnverifiedRecoverableEcdsaSignature::new(owner.sign(&signature_text).unwrap().into()),
    );
    signature_request
        .add_signature(unverified_signature, client.scw_verifier())
        .await
        .unwrap();

    client.register_identity(signature_request).await.unwrap();
}
