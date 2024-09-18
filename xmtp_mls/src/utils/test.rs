#![allow(clippy::unwrap_used)]

use rand::{
    distributions::{Alphanumeric, DistString},
    Rng, RngCore,
};
use std::sync::Arc;
use tokio::{sync::Notify, time::error::Elapsed};
use xmtp_id::associations::{
    generate_inbox_id,
    test_utils::MockSmartContractSignatureVerifier,
    unverified::{UnverifiedRecoverableEcdsaSignature, UnverifiedSignature},
};

use crate::{
    builder::ClientBuilder,
    identity::IdentityStrategy,
    storage::{EncryptedMessageStore, StorageOption},
    types::Address,
    Client, InboxOwner, XmtpApi, XmtpTestClient,
};

#[cfg(not(target_arch = "wasm32"))]
use xmtp_api_grpc::grpc_api_helper::Client as GrpcClient;
#[cfg(not(any(feature = "http-api", target_arch = "wasm32")))]
pub type TestClient = GrpcClient;

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

#[cfg(any(feature = "http-api", target_arch = "wasm32"))]
impl XmtpTestClient for XmtpHttpApiClient {
    async fn create_local() -> Self {
        XmtpHttpApiClient::new("http://localhost:5555".into()).unwrap()
    }

    async fn create_dev() -> Self {
        XmtpHttpApiClient::new("https://grpc.dev.xmtp.network:443".into()).unwrap()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl XmtpTestClient for GrpcClient {
    async fn create_local() -> Self {
        GrpcClient::create("http://localhost:5556".into(), false)
            .await
            .unwrap()
    }

    async fn create_dev() -> Self {
        GrpcClient::create("https://grpc.dev.xmtp.network:443".into(), false)
            .await
            .unwrap()
    }
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

impl ClientBuilder<TestClient> {
    pub async fn temp_store(self) -> Self {
        let tmpdb = tmp_path();
        self.store(
            EncryptedMessageStore::new(
                StorageOption::Persistent(tmpdb),
                EncryptedMessageStore::generate_enc_key(),
            )
            .await
            .unwrap(),
        )
    }

    pub async fn local_client(mut self) -> Self {
        let local_client = <TestClient as XmtpTestClient>::create_local().await;
        self = self.api_client(local_client);
        self
    }

    pub async fn new_test_client(owner: &impl InboxOwner) -> Client<TestClient> {
        let nonce = 1;
        let inbox_id = generate_inbox_id(&owner.get_address(), &nonce);

        let client = Self::new(IdentityStrategy::CreateIfNotFound(
            inbox_id,
            owner.get_address(),
            nonce,
            None,
        ))
        .scw_signatuer_verifier(MockSmartContractSignatureVerifier::new(true))
        .temp_store()
        .await
        .local_client()
        .await
        .build()
        .await
        .unwrap();

        register_client(&client, owner).await;

        client
    }

    pub async fn new_dev_client(owner: &impl InboxOwner) -> Client<TestClient> {
        let nonce = 1;
        let inbox_id = generate_inbox_id(&owner.get_address(), &nonce);
        let dev_client = <TestClient as XmtpTestClient>::create_dev().await;

        let client = Self::new(IdentityStrategy::CreateIfNotFound(
            inbox_id,
            owner.get_address(),
            nonce,
            None,
        ))
        .temp_store()
        .await
        .api_client(dev_client)
        .build()
        .await
        .unwrap();

        register_client(&client, owner).await;

        client
    }
}

/// wrapper over a `Notify` with a 60-scond timeout for waiting
#[derive(Clone, Default)]
pub struct Delivery {
    notify: Arc<Notify>,
    timeout: core::time::Duration,
}

impl Delivery {
    pub fn new(timeout: Option<core::time::Duration>) -> Self {
        let timeout = timeout.unwrap_or(core::time::Duration::from_secs(60));
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

impl Client<TestClient> {
    pub async fn is_registered(&self, address: &String) -> bool {
        let ids = self
            .api_client
            .get_inbox_ids(vec![address.clone()])
            .await
            .unwrap();
        ids.contains_key(address)
    }
}

pub async fn register_client<T: XmtpApi>(client: &Client<T>, owner: &impl InboxOwner) {
    let mut signature_request = client.context.signature_request().unwrap();
    let signature_text = signature_request.signature_text();
    let unverified_signature = UnverifiedSignature::RecoverableEcdsa(
        UnverifiedRecoverableEcdsaSignature::new(owner.sign(&signature_text).unwrap().into()),
    );
    signature_request
        .add_signature(
            unverified_signature,
            client.smart_contract_signature_verifier().as_ref(),
        )
        .await
        .unwrap();

    client.register_identity(signature_request).await.unwrap();
}
