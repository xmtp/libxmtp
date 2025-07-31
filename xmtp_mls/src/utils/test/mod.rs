#![allow(clippy::unwrap_used)]

#[cfg(any(test, feature = "test-utils"))]
pub mod tester_utils;

#[cfg(any(test, feature = "test-utils"))]
pub mod fixtures;
pub mod test_mocks_helpers;

use crate::XmtpApi;
use crate::{
    builder::{ClientBuilder, SyncWorkerMode},
    context::{XmtpMlsLocalContext, XmtpSharedContext},
    identity::IdentityStrategy,
    Client, InboxOwner,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::Notify;
use xmtp_api::ApiIdentifier;
use xmtp_common::time::Expired;
use xmtp_db::{sql_key_store::SqlKeyStore, XmtpMlsStorageProvider};
use xmtp_db::{ConnectionExt, DbConnection, XmtpTestDb};
use xmtp_id::associations::{test_utils::MockSmartContractSignatureVerifier, Identifier};
use xmtp_proto::api_client::{ApiBuilder, XmtpTestClient};

#[cfg(any(test, feature = "test-utils"))]
pub use tester_utils::*;

pub type TestMlsStorage = SqlKeyStore<xmtp_db::DefaultDbConnection>;
pub type TestXmtpMlsContext =
    Arc<XmtpMlsLocalContext<TestClient, xmtp_db::DefaultStore, TestMlsStorage>>;
pub type FullXmtpClient = Client<TestXmtpMlsContext>;
pub type TestMlsGroup = crate::groups::MlsGroup<TestXmtpMlsContext>;

#[cfg(not(any(feature = "http-api", target_arch = "wasm32")))]
pub type TestClient = xmtp_api_grpc::grpc_api_helper::Client;

#[cfg(all(
    any(feature = "http-api", target_arch = "wasm32"),
    not(feature = "d14n")
))]
use xmtp_api_http::XmtpHttpApiClient;

use super::VersionInfo;

#[cfg(all(
    any(feature = "http-api", target_arch = "wasm32"),
    not(feature = "d14n")
))]
pub type TestClient = XmtpHttpApiClient;

#[cfg(feature = "d14n")]
pub type TestClient = xmtp_api_d14n::TestD14nClient;

impl<A, S> ClientBuilder<A, S> {
    pub async fn temp_store(self) -> Self {
        self.store(xmtp_db::TestDb::create_persistent_store(None).await)
    }

    pub async fn unencrypted_store(self, path: Option<String>) -> Self {
        self.store(xmtp_db::TestDb::create_unencrypted_persistent_store(path).await)
    }

    pub async fn dev(self) -> ClientBuilder<TestClient, S> {
        let api_client = <TestClient as XmtpTestClient>::create_dev()
            .build()
            .await
            .unwrap();
        let sync_api_client = <TestClient as XmtpTestClient>::create_dev()
            .build()
            .await
            .unwrap();
        self.api_clients(api_client, sync_api_client)
    }

    pub async fn local(self) -> ClientBuilder<TestClient, S> {
        let api_client = <TestClient as XmtpTestClient>::create_local()
            .build()
            .await
            .unwrap();
        let sync_api_client = <TestClient as XmtpTestClient>::create_local()
            .build()
            .await
            .unwrap();
        self.api_clients(api_client, sync_api_client)
    }
}

impl<Api, Storage, Db> ClientBuilder<Api, Storage, Db>
where
    Api: XmtpApi + 'static + Send + Sync,
    Storage: XmtpMlsStorageProvider + 'static + Send + Sync,
    Db: xmtp_db::XmtpDb + 'static + Send + Sync,
{
    pub async fn build_unchecked(self) -> Client<Arc<XmtpMlsLocalContext<Api, Db, Storage>>> {
        self.build().await.unwrap()
    }
}

impl ClientBuilder<TestClient, TestMlsStorage> {
    pub fn local_port() -> &'static str {
        <TestClient as XmtpTestClient>::local_port()
    }

    pub async fn new_custom_api_client(addr: &str) -> TestClient {
        <TestClient as XmtpTestClient>::create_custom(addr)
            .build()
            .await
            .unwrap()
    }

    pub async fn new_test_builder(owner: &impl InboxOwner) -> ClientBuilder<(), TestMlsStorage> {
        let strategy = identity_setup(owner);
        Client::builder(strategy)
            .temp_store()
            .await
            .with_disable_events(None)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .device_sync_server_url(crate::configuration::DeviceSyncUrls::LOCAL_ADDRESS)
            .enable_sqlite_triggers()
            .default_mls_store()
            .unwrap()
    }

    pub async fn new_test_builder_with_unencrypted_store(
        owner: &impl InboxOwner,
    ) -> ClientBuilder<(), TestMlsStorage> {
        let strategy = identity_setup(owner);

        Client::builder(strategy)
            .unencrypted_store(None)
            .await
            .with_disable_events(None)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .device_sync_server_url(crate::configuration::DeviceSyncUrls::LOCAL_ADDRESS)
            .enable_sqlite_triggers()
            .default_mls_store()
            .unwrap()
    }

    pub async fn new_test_client(owner: &impl InboxOwner) -> FullXmtpClient {
        let client = Self::new_test_builder(owner)
            .await
            .local()
            .await
            .build()
            .await
            .unwrap();
        register_client(&client, owner).await;
        client
    }

    /// Test client with unencrypted permanent store
    pub async fn new_test_client_with_unencrypted_store(owner: &impl InboxOwner) -> FullXmtpClient {
        let client = Self::new_test_builder_with_unencrypted_store(owner)
            .await
            .local()
            .await
            .build()
            .await
            .unwrap();
        register_client(&client, owner).await;
        client
    }

    /// Test client without anything extra
    pub async fn new_test_client_vanilla(owner: &impl InboxOwner) -> FullXmtpClient {
        let client = Self::new_test_builder(owner)
            .await
            .local()
            .await
            .with_disable_events(Some(true))
            .device_sync_worker_mode(SyncWorkerMode::Disabled)
            .build()
            .await
            .unwrap();
        register_client(&client, owner).await;
        client
    }

    pub async fn new_test_client_with_version(
        owner: &impl InboxOwner,
        version: VersionInfo,
    ) -> FullXmtpClient {
        let client = Self::new_test_builder(owner)
            .await
            .local()
            .await
            .device_sync_worker_mode(SyncWorkerMode::Disabled)
            .version(version)
            .build()
            .await
            .unwrap();

        register_client(&client, owner).await;
        client
    }

    pub async fn new_test_client_dev(owner: &impl InboxOwner) -> FullXmtpClient {
        let api_client = <TestClient as XmtpTestClient>::create_dev()
            .build()
            .await
            .unwrap();
        let sync_api_client = <TestClient as XmtpTestClient>::create_dev()
            .build()
            .await
            .unwrap();

        let client = Self::new_test_builder(owner)
            .await
            .api_clients(api_client, sync_api_client)
            .build()
            .await
            .unwrap();

        register_client(&client, owner).await;
        client
    }

    pub async fn new_test_client_with_history(
        owner: &impl InboxOwner,
        history_sync_url: &str,
    ) -> FullXmtpClient {
        let api_client = <TestClient as XmtpTestClient>::create_local()
            .build()
            .await
            .unwrap();

        let sync_api_client = <TestClient as XmtpTestClient>::create_local()
            .build()
            .await
            .unwrap();

        let client = Self::new_test_builder(owner)
            .await
            .api_clients(api_client, sync_api_client)
            .device_sync_server_url(history_sync_url)
            .build()
            .await
            .unwrap();

        register_client(&client, owner).await;
        client
    }
}

impl<ApiClient, Db> ClientBuilder<ApiClient, Db> {
    pub async fn local_client(self) -> ClientBuilder<TestClient, Db> {
        self.api_clients(
            <TestClient as XmtpTestClient>::create_local()
                .build()
                .await
                .unwrap(),
            <TestClient as XmtpTestClient>::create_local()
                .build()
                .await
                .unwrap(),
        )
    }

    pub async fn dev_client(self) -> ClientBuilder<TestClient, Db> {
        self.api_clients(
            <TestClient as XmtpTestClient>::create_dev()
                .build()
                .await
                .unwrap(),
            <TestClient as XmtpTestClient>::create_dev()
                .build()
                .await
                .unwrap(),
        )
    }
}

fn identity_setup(owner: impl InboxOwner) -> IdentityStrategy {
    let nonce = 1;
    let ident = owner.get_identifier().unwrap();
    let inbox_id = ident.inbox_id(nonce).unwrap();
    IdentityStrategy::new(inbox_id, ident, nonce, None)
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

impl<Context> Client<Context>
where
    Context: XmtpSharedContext,
{
    pub async fn is_registered(&self, identifier: &Identifier) -> bool {
        let identifier: ApiIdentifier = identifier.into();
        let ids = self
            .context
            .api()
            .get_inbox_ids(vec![identifier.clone()])
            .await
            .unwrap();
        ids.contains_key(&identifier)
    }
}

pub async fn register_client<Context: XmtpSharedContext>(
    client: &Client<Context>,
    owner: impl InboxOwner,
) {
    let mut signature_request = client.context.signature_request().unwrap();
    let signature_text = signature_request.signature_text();
    let unverified_signature = owner.sign(&signature_text).unwrap();

    signature_request
        .add_signature(unverified_signature, client.scw_verifier())
        .await
        .unwrap();

    client.register_identity(signature_request).await.unwrap();
}

/// wait for a minimum amount of intents to be published
/// TODO: Should wrap with a timeout
pub async fn wait_for_min_intents<C: ConnectionExt>(
    conn: &DbConnection<C>,
    n: usize,
) -> Result<(), Expired> {
    let mut published = conn.intents_published() as usize;
    xmtp_common::time::timeout(Duration::from_secs(5), async {
        while published < n {
            xmtp_common::task::yield_now().await;
            published = conn.intents_published() as usize;
        }
    })
    .await
}
