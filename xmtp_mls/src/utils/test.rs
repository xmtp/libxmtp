use std::env;

use rand::{
    distributions::{Alphanumeric, DistString},
    Rng,
};
use std::sync::Arc;
use tokio::{sync::Notify, time::error::Elapsed};
use xmtp_api_grpc::grpc_api_helper::Client as GrpcClient;
use xmtp_id::associations::{generate_inbox_id, RecoverableEcdsaSignature};

use crate::{
    builder::ClientBuilder,
    identity::IdentityStrategy,
    storage::{EncryptedMessageStore, StorageOption},
    types::Address,
    Client, InboxOwner, XmtpApi, XmtpTestClient,
};

#[cfg(feature = "http-api")]
use xmtp_api_http::XmtpHttpApiClient;

#[cfg(not(feature = "http-api"))]
pub type TestClient = GrpcClient;

#[cfg(feature = "http-api")]
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

pub fn tmp_path() -> String {
    let db_name = rand_string();
    format!("{}/{}.db3", env::temp_dir().to_str().unwrap(), db_name)
}

pub fn rand_time() -> i64 {
    let mut rng = rand::thread_rng();
    rng.gen_range(0..1_000_000_000)
}

#[async_trait::async_trait]
#[cfg(feature = "http-api")]
impl XmtpTestClient for XmtpHttpApiClient {
    async fn create_local() -> Self {
        XmtpHttpApiClient::new("http://localhost:5555".into()).unwrap()
    }

    async fn create_dev() -> Self {
        XmtpHttpApiClient::new("https://grpc.dev.xmtp.network:443".into()).unwrap()
    }
}

#[async_trait::async_trait]
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

impl ClientBuilder<TestClient> {
    pub fn temp_store(self) -> Self {
        let tmpdb = tmp_path();
        self.store(
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb)).unwrap(),
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
        .temp_store()
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
    timeout: std::time::Duration,
}

impl Delivery {
    pub fn new(timeout: Option<std::time::Duration>) -> Self {
        let timeout = timeout.unwrap_or(std::time::Duration::from_secs(60));
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
    signature_request
        .add_signature(Box::new(RecoverableEcdsaSignature::new(
            signature_text.clone(),
            owner.sign(&signature_text).unwrap().into(),
        )))
        .await
        .unwrap();

    client.register_identity(signature_request).await.unwrap();
}
