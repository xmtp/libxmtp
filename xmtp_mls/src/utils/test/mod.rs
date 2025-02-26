#![allow(clippy::unwrap_used)]

use std::{
    future::Future,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::sync::Notify;
use xmtp_common::time::{timeout, Expired};
use xmtp_id::{
    associations::{
        generate_inbox_id,
        test_utils::MockSmartContractSignatureVerifier,
        unverified::{UnverifiedRecoverableEcdsaSignature, UnverifiedSignature},
    },
    scw_verifier::{RemoteSignatureVerifier, SmartContractSignatureVerifier},
};
use xmtp_proto::api_client::XmtpTestClient;

use crate::{
    builder::ClientBuilder,
    identity::IdentityStrategy,
    storage::{DbConnection, EncryptedMessageStore, StorageOption},
    Client, InboxOwner, XmtpApi,
};

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

impl EncryptedMessageStore {
    pub fn generate_enc_key() -> [u8; 32] {
        xmtp_common::rand_array::<32>()
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
        let tmpdb = xmtp_common::tmp_path();
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

    pub async fn new_test_client_dev(owner: &impl InboxOwner) -> FullXmtpClient {
        let api_client = <TestClient as XmtpTestClient>::create_dev().await;

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

impl<ApiClient, V> ClientBuilder<ApiClient, V> {
    pub async fn local_client(self) -> ClientBuilder<TestClient, V> {
        self.api_client(<TestClient as XmtpTestClient>::create_local().await)
    }

    pub async fn dev_client(self) -> ClientBuilder<TestClient, V> {
        self.api_client(<TestClient as XmtpTestClient>::create_dev().await)
    }
}

impl ClientBuilder<TestClient, RemoteSignatureVerifier<TestClient>> {
    /// Create a client pointed at the local container with the default remote verifier
    pub async fn new_local_client(owner: &impl InboxOwner) -> Client<TestClient> {
        let api_client = <TestClient as XmtpTestClient>::create_local().await;
        inner_build(owner, api_client).await
    }

    pub async fn new_dev_client(owner: &impl InboxOwner) -> Client<TestClient> {
        let api_client = <TestClient as XmtpTestClient>::create_dev().await;
        inner_build(owner, api_client).await
    }
}

async fn inner_build<A>(owner: impl InboxOwner, api_client: A) -> Client<A>
where
    A: XmtpApi + 'static + Send + Sync + Clone,
{
    let nonce = 1;
    let inbox_id = generate_inbox_id(&owner.get_address(), &nonce).unwrap();

    let client = Client::builder(IdentityStrategy::new(
        inbox_id,
        owner.get_address(),
        nonce,
        None,
    ));

    let client = client
        .temp_store()
        .await
        .api_client(api_client)
        .with_remote_verifier()
        .unwrap()
        .build()
        .await
        .unwrap();
    let conn = client.store().conn().unwrap();
    conn.register_triggers();
    conn.disable_memory_security();
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

    let mut builder = Client::builder(IdentityStrategy::new(
        inbox_id,
        owner.get_address(),
        nonce,
        None,
    ))
    .temp_store()
    .await
    .api_client(api_client)
    .with_scw_verifier(scw_verifier);

    if let Some(history_sync_url) = history_sync_url {
        builder = builder.history_sync_url(history_sync_url);
    }

    let client = builder.build().await.unwrap();
    let conn = client.store().conn().unwrap();
    conn.register_triggers();
    conn.disable_memory_security();
    register_client(&client, owner).await;

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

    pub async fn wait_for_delivery(&self) -> Result<(), xmtp_common::time::Expired> {
        xmtp_common::time::timeout(self.timeout, async { self.notify.notified().await }).await
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

#[derive(Default)]
pub struct WorkerHandle {
    processed: AtomicUsize,
    notify: Notify,
}

impl WorkerHandle {
    pub async fn wait_for_new_events(&self, mut count: usize) -> Result<(), Expired> {
        timeout(xmtp_common::time::Duration::from_secs(3), async {
            while count > 0 {
                self.notify.notified().await;
                count -= 1;
            }
        })
        .await?;

        Ok(())
    }

    pub async fn wait_for_processed_count(&self, expected: usize) -> Result<(), Expired> {
        timeout(xmtp_common::time::Duration::from_secs(3), async {
            while self.processed.load(Ordering::SeqCst) < expected {
                self.notify.notified().await;
            }
        })
        .await?;

        Ok(())
    }

    pub async fn block_for_num_events<Fut>(&self, num_events: usize, op: Fut) -> Result<(), Expired>
    where
        Fut: Future<Output = ()>,
    {
        let processed_count = self.processed_count();
        op.await;
        self.wait_for_processed_count(processed_count + num_events)
            .await?;
        Ok(())
    }

    pub fn processed_count(&self) -> usize {
        self.processed.load(Ordering::SeqCst)
    }

    pub fn increment(&self) {
        self.processed
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.notify.notify_waiters();
    }
}

impl<ApiClient, V> Client<ApiClient, V> {
    pub fn sync_worker_handle(&self) -> Option<Arc<WorkerHandle>> {
        self.sync_worker_handle.lock().clone()
    }

    pub(crate) fn set_sync_worker_handle(&self, handle: Arc<WorkerHandle>) {
        *self.sync_worker_handle.lock() = Some(handle);
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

/// wait for a minimum amount of intents to be published
/// TODO: Should wrap with a timeout
pub async fn wait_for_min_intents(conn: &DbConnection, n: usize) {
    let mut published = conn.intents_published() as usize;
    while published < n {
        xmtp_common::yield_().await;
        published = conn.intents_published() as usize;
    }
}

#[cfg(any(test, feature = "test-utils"))]
/// Checks if test mode is enabled.
pub fn is_test_mode_upload_malformed_keypackage() -> bool {
    use std::env;
    env::var("TEST_MODE_UPLOAD_MALFORMED_KP").unwrap_or_else(|_| "false".to_string()) == "true"
}

#[cfg(any(test, feature = "test-utils"))]
#[warn(dead_code)]
/// Sets test mode and specifies malformed installations dynamically.
/// If `enable` is `false`, it also clears `TEST_MODE_MALFORMED_INSTALLATIONS`.
pub fn set_test_mode_upload_malformed_keypackage(
    enable: bool,
    installations: Option<Vec<Vec<u8>>>,
) {
    use std::env;
    if enable {
        env::set_var("TEST_MODE_UPLOAD_MALFORMED_KP", "true");
        env::remove_var("TEST_MODE_MALFORMED_INSTALLATIONS");

        if let Some(installs) = installations {
            let installations_str = installs
                .iter()
                .map(hex::encode)
                .collect::<Vec<_>>()
                .join(",");

            env::set_var("TEST_MODE_MALFORMED_INSTALLATIONS", installations_str);
        }
    } else {
        env::set_var("TEST_MODE_UPLOAD_MALFORMED_KP", "false");
        env::remove_var("TEST_MODE_MALFORMED_INSTALLATIONS");
    }
}

#[cfg(any(test, feature = "test-utils"))]
/// Retrieves and decodes malformed installations from the environment variable.
/// Returns an empty list if test mode is not enabled.
pub fn get_test_mode_malformed_installations() -> Vec<Vec<u8>> {
    use std::env;
    if !is_test_mode_upload_malformed_keypackage() {
        return Vec::new();
    }

    env::var("TEST_MODE_MALFORMED_INSTALLATIONS")
        .unwrap_or_else(|_| "".to_string())
        .split(',')
        .filter_map(|s| {
            if s.is_empty() {
                None
            } else {
                Some(hex::decode(s).unwrap_or_else(|_| Vec::new()))
            }
        })
        .collect()
}
