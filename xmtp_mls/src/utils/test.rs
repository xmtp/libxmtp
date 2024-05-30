use std::env;

use rand::{
    distributions::{Alphanumeric, DistString},
    Rng,
};
use xmtp_api_grpc::grpc_api_helper::Client as GrpcClient;
use xmtp_id::associations::RecoverableEcdsaSignature;

use crate::{
    builder::ClientBuilder,
    identity::IdentityStrategy,
    storage::{EncryptedMessageStore, StorageOption},
    types::Address,
    Client, InboxOwner,
};

pub type TestClient = Client<GrpcClient>;

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

/// Get a GRPC Client pointed at the local instance of `xmtp-node-go`
pub async fn get_local_grpc_client() -> GrpcClient {
    GrpcClient::create("http://localhost:5556".to_string(), false)
        .await
        .unwrap()
}

impl ClientBuilder<GrpcClient> {
    pub async fn local_grpc(self) -> Self {
        self.api_client(get_local_grpc_client().await)
    }

    pub fn temp_store(self) -> Self {
        let tmpdb = tmp_path();
        self.store(
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb)).unwrap(),
        )
    }

    pub async fn new_test_client(owner: &impl InboxOwner) -> Client<GrpcClient> {
        let client = Self::new(IdentityStrategy::CreateIfNotFound(
            owner.get_address(),
            None,
        ))
        .temp_store()
        .local_grpc()
        .await
        .build()
        .await
        .unwrap();

        register_client(&client, owner).await;

        client
    }
}

impl Client<GrpcClient> {
    pub async fn is_registered(&self, address: &String) -> bool {
        let ids = self
            .api_client
            .get_inbox_ids(vec![address.clone()])
            .await
            .unwrap();
        ids.contains_key(address)
    }
}

pub async fn register_client(client: &Client<GrpcClient>, owner: &impl InboxOwner) {
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
