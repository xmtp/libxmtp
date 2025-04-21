#![allow(clippy::unwrap_used)]

#[cfg(any(test, feature = "test-utils"))]
pub mod tester_utils;

use crate::{
    builder::{ClientBuilder, SyncWorkerMode},
    identity::IdentityStrategy,
    Client, InboxOwner, XmtpApi,
};
use openmls::group::{ProcessMessageError, ValidationError::WrongEpoch};
use std::{sync::Arc, time::Duration};
use tokio::sync::Notify;
use xmtp_api::ApiIdentifier;
use xmtp_common::time::Expired;
use xmtp_db::{ConnectionExt, DbConnection, XmtpTestDb};
use xmtp_id::{
    associations::{test_utils::MockSmartContractSignatureVerifier, Identifier},
    scw_verifier::{RemoteSignatureVerifier, SmartContractSignatureVerifier},
};
use xmtp_proto::api_client::{ApiBuilder, XmtpTestClient};

#[cfg(any(test, feature = "test-utils"))]
pub use tester_utils::*;

pub type FullXmtpClient = Client<TestClient>;

#[cfg(not(any(feature = "http-api", target_arch = "wasm32")))]
pub type TestClient = xmtp_api_grpc::grpc_api_helper::Client;

use crate::groups::mls_sync::GroupMessageProcessingError;
use crate::groups::mls_sync::GroupMessageProcessingError::OpenMlsProcessMessage;
#[cfg(all(
    any(feature = "http-api", target_arch = "wasm32"),
    not(feature = "d14n")
))]
use xmtp_api_http::XmtpHttpApiClient;

#[cfg(all(
    any(feature = "http-api", target_arch = "wasm32"),
    not(feature = "d14n")
))]
pub type TestClient = XmtpHttpApiClient;

#[cfg(feature = "d14n")]
pub type TestClient = xmtp_api_d14n::TestD14nClient;

impl<A> ClientBuilder<A> {
    pub async fn temp_store(self) -> Self {
        self.store(xmtp_db::TestDb::create_persistent_store(None).await)
    }
}

impl ClientBuilder<TestClient> {
    pub async fn new_api_client() -> TestClient {
        <TestClient as XmtpTestClient>::create_local()
            .build()
            .await
            .unwrap()
    }

    pub async fn new_test_client(owner: &impl InboxOwner) -> FullXmtpClient {
        let api_client = Self::new_api_client().await;

        build_with_verifier(
            owner,
            api_client,
            MockSmartContractSignatureVerifier::new(true),
            Some(crate::configuration::DeviceSyncUrls::LOCAL_ADDRESS),
            None,
        )
        .await
    }

    pub async fn new_test_client_no_sync(owner: &impl InboxOwner) -> FullXmtpClient {
        let api_client = <TestClient as XmtpTestClient>::create_local()
            .build()
            .await
            .unwrap();

        build_with_verifier(
            owner,
            api_client,
            MockSmartContractSignatureVerifier::new(true),
            None,
            Some(SyncWorkerMode::Disabled),
        )
        .await
    }

    pub async fn new_test_client_dev(owner: &impl InboxOwner) -> FullXmtpClient {
        let api_client = <TestClient as XmtpTestClient>::create_dev()
            .build()
            .await
            .unwrap();

        build_with_verifier(
            owner,
            api_client,
            MockSmartContractSignatureVerifier::new(true),
            None,
            None,
        )
        .await
    }

    pub async fn new_test_client_with_history(
        owner: &impl InboxOwner,
        history_sync_url: &str,
    ) -> FullXmtpClient {
        let api_client = <TestClient as XmtpTestClient>::create_local()
            .build()
            .await
            .unwrap();

        build_with_verifier(
            owner,
            api_client,
            MockSmartContractSignatureVerifier::new(true),
            Some(history_sync_url),
            None,
        )
        .await
    }

    /// A client pointed at the dev network with a Mock verifier (never fail to verify)
    pub async fn new_mock_dev_client(owner: impl InboxOwner) -> Client<TestClient> {
        let api_client = <TestClient as XmtpTestClient>::create_dev()
            .build()
            .await
            .unwrap();

        build_with_verifier(
            owner,
            api_client,
            MockSmartContractSignatureVerifier::new(true),
            None,
            None,
        )
        .await
    }
}

impl<ApiClient, Db> ClientBuilder<ApiClient, Db> {
    pub async fn local_client(self) -> ClientBuilder<TestClient, Db> {
        self.api_client(
            <TestClient as XmtpTestClient>::create_local()
                .build()
                .await
                .unwrap(),
        )
    }

    pub async fn dev_client(self) -> ClientBuilder<TestClient, Db> {
        self.api_client(
            <TestClient as XmtpTestClient>::create_dev()
                .build()
                .await
                .unwrap(),
        )
    }
}

impl ClientBuilder<TestClient, RemoteSignatureVerifier<TestClient>> {
    /// Create a client pointed at the local container with the default remote verifier
    pub async fn new_local_client(owner: &impl InboxOwner) -> Client<TestClient> {
        let api_client = <TestClient as XmtpTestClient>::create_local()
            .build()
            .await
            .unwrap();
        inner_build(owner, api_client).await
    }

    pub async fn new_dev_client(owner: &impl InboxOwner) -> Client<TestClient> {
        let api_client = <TestClient as XmtpTestClient>::create_dev()
            .build()
            .await
            .unwrap();
        inner_build(owner, api_client).await
    }
}

async fn inner_build<A>(owner: impl InboxOwner, api_client: A) -> Client<A>
where
    A: XmtpApi + 'static + Send + Sync + Clone,
{
    let nonce = 1;
    let ident = owner.get_identifier().unwrap();
    let inbox_id = ident.inbox_id(nonce).unwrap();

    let client = Client::builder(IdentityStrategy::new(inbox_id, ident, nonce, None));

    let client = client
        .temp_store()
        .await
        .api_client(api_client)
        .with_remote_verifier()
        .unwrap()
        .build()
        .await
        .unwrap();
    let conn = client.context.db();
    conn.register_triggers();
    conn.disable_memory_security();
    register_client(&client, owner).await;

    client
}

async fn build_with_verifier<A, V>(
    owner: impl InboxOwner,
    api_client: A,
    scw_verifier: V,
    sync_server_url: Option<&str>,
    sync_worker_mode: Option<SyncWorkerMode>,
) -> Client<A>
where
    A: XmtpApi + Send + Sync + 'static,
    V: SmartContractSignatureVerifier + Send + Sync + 'static,
{
    let nonce = 1;
    let ident = owner.get_identifier().unwrap();
    let inbox_id = ident.inbox_id(nonce).unwrap();

    let mut builder = Client::builder(IdentityStrategy::new(inbox_id, ident, nonce, None))
        .temp_store()
        .await
        .api_client(api_client)
        .with_scw_verifier(scw_verifier);

    if let Some(sync_server_url) = sync_server_url {
        builder = builder.device_sync_server_url(sync_server_url);
    }

    if let Some(sync_worker_mode) = sync_worker_mode {
        builder = builder.device_sync_worker_mode(sync_worker_mode);
    }

    let client = builder.build().await.unwrap();
    let conn = client.context.db();
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

impl<ApiClient, Db> Client<ApiClient, Db>
where
    ApiClient: XmtpApi,
{
    pub async fn is_registered(&self, identifier: &Identifier) -> bool {
        let identifier: ApiIdentifier = identifier.into();
        let ids = self
            .api_client
            .get_inbox_ids(vec![identifier.clone()])
            .await
            .unwrap();
        ids.contains_key(&identifier)
    }
}

pub async fn register_client<T: XmtpApi>(client: &Client<T>, owner: impl InboxOwner) {
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
            xmtp_common::yield_().await;
            published = conn.intents_published() as usize;
        }
    })
    .await
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

#[cfg(any(test, feature = "test-utils"))]
/// Sets test mode to mimic future wrong epoch state.
pub fn set_test_mode_future_wrong_epoch(enable: bool) {
    use std::env;
    if enable {
        env::set_var("TEST_MODE_FUTURE_WRONG_EPOCH", "true");
    } else {
        env::set_var("TEST_MODE_FUTURE_WRONG_EPOCH", "false");
    }
}
#[cfg(any(test, feature = "test-utils"))]
/// Checks if test mode is enabled.
pub fn is_test_mode_future_wrong_epoch() -> bool {
    use std::env;
    env::var("TEST_MODE_FUTURE_WRONG_EPOCH").unwrap_or_else(|_| "false".to_string()) == "true"
}

pub fn maybe_mock_wrong_epoch_for_tests() -> Result<(), GroupMessageProcessingError> {
    if is_test_mode_future_wrong_epoch() {
        return Err(OpenMlsProcessMessage(ProcessMessageError::ValidationError(
            WrongEpoch,
        )));
    }
    Ok(())
}

pub fn maybe_mock_future_epoch_for_tests() -> Result<(), GroupMessageProcessingError> {
    if is_test_mode_future_wrong_epoch() {
        return Err(GroupMessageProcessingError::FutureEpoch(10, 0));
    }
    Ok(())
}
