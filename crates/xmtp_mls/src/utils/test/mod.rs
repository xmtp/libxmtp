#![allow(clippy::unwrap_used)]

#[cfg(all(test, not(target_arch = "wasm32")))]
pub mod backend;

#[cfg(any(test, feature = "test-utils"))]
pub mod tester_utils;

#[cfg(any(test, feature = "test-utils"))]
pub mod fixtures;

#[cfg(test)]
pub(crate) mod crafted_welcome;
pub mod test_mocks_helpers;
mod tester_utils_trait_ext;
pub use tester_utils_trait_ext::*;

use crate::XmtpApi;
use crate::{
    Client, InboxOwner,
    builder::{ClientBuilder, DeviceSyncMode},
    context::{XmtpMlsLocalContext, XmtpSharedContext},
    groups::{GroupError, MlsGroup},
    identity::IdentityStrategy,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::Notify;
use xmtp_common::time::Expired;
use xmtp_db::XmtpMlsStorageProvider;
use xmtp_db::{ConnectionExt, DbConnection, XmtpTestDb};
use xmtp_id::associations::{Identifier, test_utils::MockSmartContractSignatureVerifier};
use xmtp_proto::api_client::{ApiBuilder, XmtpTestClient};
use xmtp_proto::types::ApiIdentifier;

#[cfg(any(test, feature = "test-utils"))]
pub use tester_utils::*;
mod definitions;
pub use definitions::*;

use super::VersionInfo;

/// Each secret that must stay in the client, in the encodings a leak would
/// take: the group root and each private or symmetric key of the group key
/// table, as raw bytes and as lowercase and uppercase hex. The keys derive
/// here from the spec's salt and `info` labels, not through `GroupKeys`.
#[cfg(test)]
pub(crate) fn group_secret_encodings(
    root: &xmtp_mls_common::group_keys::GroupRoot,
) -> Vec<(String, Vec<u8>)> {
    let hkdf = hkdf::Hkdf::<sha2::Sha256>::new(Some(b"xmtp group root"), root.as_bytes());
    let mut secrets = vec![("group root".to_string(), root.as_bytes().to_vec())];
    for info in [
        "xmtp group signing key",
        "xmtp commit log signing key",
        "xmtp commit log encryption key",
        "xmtp commit log nonce key",
    ] {
        let mut key = [0u8; 32];
        hkdf.expand(info.as_bytes(), &mut key).unwrap();
        secrets.push((info.to_string(), key.to_vec()));
    }
    secrets
        .into_iter()
        .flat_map(|(name, bytes)| {
            [
                (format!("{name} as hex"), hex::encode(&bytes).into_bytes()),
                (
                    format!("{name} as upper-case hex"),
                    hex::encode_upper(&bytes).into_bytes(),
                ),
                (name, bytes),
            ]
        })
        .collect()
}

impl<A, S> ClientBuilder<A, S> {
    pub async fn temp_store(self) -> Self {
        self.store(xmtp_db::TestDb::create_persistent_store(None).await)
    }

    pub fn dev(self) -> ClientBuilder<TestClient, S> {
        self.local()
    }

    pub fn local(self) -> ClientBuilder<TestClient, S> {
        self.api_client_with_streams(Arc::new(
            DefaultTestClientCreator::create().build().unwrap(),
        ))
    }
}

impl<Api, Storage, Db> ClientBuilder<Api, Storage, Db>
where
    Api: XmtpApi + 'static,
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
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
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
            .device_sync_worker_mode(DeviceSyncMode::Disabled)
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
            .device_sync_worker_mode(DeviceSyncMode::Disabled)
            .version(version)
            .build()
            .await
            .unwrap();

        register_client(&client, owner).await;
        client
    }
}

pub fn identity_setup(owner: impl InboxOwner) -> IdentityStrategy {
    let nonce = 1;
    let ident = owner.get_identifier().unwrap();
    let inbox_id = ident.inbox_id(nonce).unwrap();
    IdentityStrategy::new(inbox_id, ident, nonce)
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
        ids.first().is_some_and(Option::is_some)
    }

    /// Sync welcomes until at least one new group arrives, and return the new groups.
    ///
    /// One `sync_welcomes` call can miss a welcome that was sent just before it. The call
    /// targets the newest welcome position that the backend reads from a replica, and the
    /// replica can lag the primary (`docs/specs/PROC-message-processing.md`, known
    /// limitations). Use this helper when a test needs a welcome that it just sent.
    ///
    /// A `sync_welcomes` error returns at once. Panics if no welcome arrives in 20 s.
    pub async fn wait_for_welcomes(&self) -> Result<Vec<MlsGroup<Context>>, GroupError> {
        let poll = async {
            loop {
                let groups = self.sync_welcomes().await?;
                if !groups.is_empty() {
                    return Ok(groups);
                }
                xmtp_common::task::yield_now().await;
            }
        };
        xmtp_common::time::timeout(Duration::from_secs(20), poll)
            .await
            .expect("no welcome arrived within 20 s")
    }

    /// Assert that this installation rejected its newest published Welcome
    /// for good: the last terminal rejection names it with `invalid_welcome`
    /// and its pending row is gone.
    pub async fn assert_newest_welcome_rejected(&self) {
        use xmtp_db::incoming_envelope::{NetworkEntityKind, QueryIncomingEnvelope, StreamTopic};

        let topic = StreamTopic {
            entity_id: self.context.installation_id().to_vec(),
            kind: NetworkEntityKind::Welcome,
        };
        let newest = self
            .context
            .api()
            .query_welcome_messages(self.context.installation_id())
            .await
            .unwrap()
            .into_iter()
            .map(|welcome| welcome.cursor)
            .max()
            .expect("a published Welcome");
        let db = self.context.db();
        let rejected = db
            .read_last_rejection(&topic)
            .unwrap()
            .expect("a terminal rejection");
        assert_eq!(rejected.sequence_id, newest);
        assert_eq!(rejected.code, "invalid_welcome");
        assert!(db.pending_envelope(&topic, newest).unwrap().is_none());
    }
}

pub async fn register_client<Context: XmtpSharedContext>(
    client: &Client<Context>,
    owner: impl InboxOwner,
) {
    client.register_with_owner(&owner).await.unwrap();
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

#[cfg(test)]
impl<Context: XmtpSharedContext> MlsGroup<Context> {
    /// Sign `message` under the group signing key of this group's root, as a
    /// member signs it at preparation. A test that changes a group message
    /// signs it again, so that admission accepts the changed bytes and the
    /// change reaches the receiving client.
    pub(crate) fn sign_as_member(&self, message: &mut xmtp_proto::backend_v1::GroupMessage) {
        let keys = self
            .with_group_snapshot(|group| {
                Ok(xmtp_mls_common::app_data::group_root::check_group_root(
                    group.extensions(),
                    group.group_id().as_slice(),
                ))
            })
            .unwrap()
            .expect("a keyed group holds the root of its id");
        xmtp_mls_common::group_keys::sign_group_message(&keys, message)
            .expect("signing passes the self-check");
    }
}

/// Set a pending receipt to test registration recovery after reopen.
pub fn set_registration_cursor_for_test<C: ConnectionExt>(db: &DbConnection<C>, sequence: i64) {
    use xmtp_db::diesel::{self, ExpressionMethods, RunQueryDsl};
    use xmtp_db::schema::identity::dsl;
    db.raw_query(|conn| {
        diesel::update(dsl::identity)
            .set(dsl::registration_cursor_sequence_id.eq(Some(sequence)))
            .execute(conn)
    })
    .unwrap();
}
