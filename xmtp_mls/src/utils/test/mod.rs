#![allow(clippy::unwrap_used)]

#[cfg(any(test, feature = "test-utils"))]
pub mod tester_utils;

#[cfg(any(test, feature = "test-utils"))]
pub mod fixtures;
pub mod test_mocks_helpers;
mod tester_utils_trait_ext;
pub use tester_utils_trait_ext::*;

use crate::XmtpApi;
use crate::cursor_store::SqliteCursorStore;
use crate::{
    Client, InboxOwner,
    builder::{ClientBuilder, SyncWorkerMode},
    context::{XmtpMlsLocalContext, XmtpSharedContext},
    identity::IdentityStrategy,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::Notify;
use xmtp_api_d14n::XmtpTestClientExt;
use xmtp_api_d14n::protocol::XmtpQuery;
use xmtp_common::time::Expired;
use xmtp_db::XmtpMlsStorageProvider;
use xmtp_db::{ConnectionExt, DbConnection, XmtpTestDb};
use xmtp_id::associations::{Identifier, test_utils::MockSmartContractSignatureVerifier};
use xmtp_proto::api_client::ApiBuilder;
use xmtp_proto::types::ApiIdentifier;

#[cfg(any(test, feature = "test-utils"))]
pub use tester_utils::*;
mod definitions;
pub use definitions::*;

use super::VersionInfo;

impl<A, S> ClientBuilder<A, S> {
    pub async fn temp_store(self) -> Self {
        self.store(xmtp_db::TestDb::create_persistent_store(None).await)
    }

    pub fn dev(self) -> ClientBuilder<TestClient, S> {
        let s = Arc::new(SqliteCursorStore::new(self.store.as_ref().unwrap().db()));
        let a = DevOnlyTestClientCreator::with_cursor_store(s.clone());
        let s = DevOnlyTestClientCreator::with_cursor_store(s);
        let api_client = a.build().unwrap();
        let sync_api_client = s.build().unwrap();
        self.api_clients(api_client, sync_api_client)
    }

    pub fn local(self) -> ClientBuilder<TestClient, S> {
        let s = Arc::new(SqliteCursorStore::new(self.store.as_ref().unwrap().db()));
        let a = LocalOnlyTestClientCreator::with_cursor_store(s.clone());
        let s = LocalOnlyTestClientCreator::with_cursor_store(s);
        let api_client = a.build().unwrap();
        let sync_api_client = s.build().unwrap();
        self.api_clients(api_client, sync_api_client)
    }
}

impl<Api, Storage, Db> ClientBuilder<Api, Storage, Db>
where
    Api: XmtpApi + XmtpQuery + 'static,
    Storage: XmtpMlsStorageProvider + 'static,
    Db: xmtp_db::XmtpDb + 'static,
{
    pub async fn build_unchecked(self) -> Client<Arc<XmtpMlsLocalContext<Api, Db, Storage>>> {
        self.build().await.unwrap()
    }
}

impl ClientBuilder<TestClient, TestMlsStorage> {
    pub async fn new_test_builder(
        owner: &impl InboxOwner,
    ) -> ClientBuilder<TestClient, TestMlsStorage> {
        let strategy = identity_setup(owner);
        Client::builder(strategy)
            .temp_store()
            .await
            .with_disable_events(None)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .device_sync_server_url(xmtp_configuration::DeviceSyncUrls::LOCAL_ADDRESS)
            .enable_sqlite_triggers()
            .default_mls_store()
            .unwrap()
            .local()
    }

    pub async fn new_test_client(owner: &impl InboxOwner) -> FullXmtpClient {
        let client = Self::new_test_builder(owner).await.build().await.unwrap();
        register_client(&client, owner).await;
        client
    }

    /// Test client without anything extra
    pub async fn new_test_client_vanilla(owner: &impl InboxOwner) -> FullXmtpClient {
        let client = Self::new_test_builder(owner)
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
            .device_sync_worker_mode(SyncWorkerMode::Disabled)
            .version(version)
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
        let client = Self::new_test_builder(owner)
            .await
            .device_sync_server_url(history_sync_url)
            .build()
            .await
            .unwrap();

        register_client(&client, owner).await;
        client
    }
}

fn identity_setup(owner: impl InboxOwner) -> IdentityStrategy {
    let nonce = 1;
    let ident = owner.get_identifier().unwrap();
    let inbox_id = ident.inbox_id(nonce).unwrap();
    IdentityStrategy::new(inbox_id, ident, nonce, None)
}

/// wrapper over a `Notify` with a 60-second timeout for waiting
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
