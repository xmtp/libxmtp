#![allow(clippy::unwrap_used)]
use std::env;

use rand::{
    distributions::{Alphanumeric, DistString},
    Rng,
};
use std::sync::Arc;
use tokio::{sync::Notify, time::error::Elapsed};
use xmtp_api_grpc::grpc_api_helper::Client as GrpcClient;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_id::associations::{generate_inbox_id, RecoverableEcdsaSignature};

use crate::{
    builder::ClientBuilder,
    groups::{GroupMetadataOptions, MlsGroup},
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

/// Create a bunch of random clients
pub async fn create_bulk_clients(num: usize) -> Vec<Arc<Client<TestClient>>> {
    let mut futures = vec![];
    for _ in 0..num {
        futures.push(async move {
            let local = generate_local_wallet();
            Arc::new(ClientBuilder::new_test_client(&local).await)
        });
    }
    futures::future::join_all(futures).await
}

pub async fn create_groups(
    client: &Client<TestClient>,
    peers: &[Arc<Client<TestClient>>],
    num_groups: usize,
    num_msgs: usize,
) -> Result<Vec<MlsGroup>, anyhow::Error> {
    let mut groups = vec![];
    let ids = peers.iter().map(|p| p.inbox_id()).collect::<Vec<String>>();

    for index in 0..num_groups {
        let group = client.create_group(
            None,
            GroupMetadataOptions {
                name: Some(format!("group {index}")),
                image_url_square: Some(format!("www.group{index}.com")),
                description: Some(format!("group {index}")),
                ..Default::default()
            },
        )?;
        group.add_members_by_inbox_id(client, ids.clone()).await?;
        for msg_index in 0..num_msgs {
            group
                .send_message(format!("Alix message {msg_index}").as_bytes(), client)
                .await?;
        }
        groups.push(group);
    }
    Ok(groups)
}

pub async fn create_messages<S: AsRef<str>>(
    group: &MlsGroup,
    client: &Client<TestClient>,
    num_msgs: usize,
    name: S,
) -> Result<usize, anyhow::Error> {
    let mut messages = 0;
    let name = name.as_ref();
    for msg_index in 0..num_msgs {
        group
            .send_message(format!("{name} Message {msg_index}").as_bytes(), client)
            .await?;
        messages += 1;
    }
    Ok(messages)
}
