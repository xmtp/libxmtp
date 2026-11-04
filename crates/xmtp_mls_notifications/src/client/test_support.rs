//! Notification configuration, task identity, and a backend whose
//! notification requests a scripted peer answers, for tests outside the
//! notification modules.

use super::{NotificationError, NotificationState};
use parking_lot::Mutex;
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::Notify;
use xmtp_api::{ApiClientWrapper, XmtpApi};
use xmtp_db::{DefaultDbConnection, tasks::TaskDataHash};
use xmtp_mls_core::notifications::{NotificationChannel, NotificationConfig};
use xmtp_mls_core::{context::CoreContext, policy::StreamPolicy};
use xmtp_mls_engine::groups::test_support::{EngineTestFeatures, EngineTestPeer};
use xmtp_mls_engine::groups::test_support::{PeerApi, PeerFeatures, TestPeer};
use xmtp_proto::{api::ApiClientError, api_client::XmtpBackendClient, backend_v1 as wire};

/// An HTTP channel for tests that read only the peer's subscriptions.
pub fn config() -> NotificationConfig {
    NotificationConfig::new(NotificationChannel::Http {
        url: "https://127.0.0.1/push".into(),
        signing_key: vec![7; 32].into(),
    })
}

/// The data hash of the notification runner's task.
pub fn task_hash() -> TaskDataHash {
    crate::worker::task_hash()
}

/// The notification task's first retry backoff.
pub const RETRY_NS: i64 = crate::worker::RETRY_NS;

/// The lifetime of a registration the scripted peer confirms.
pub const TEST_TTL_NS: i64 = 4 * xmtp_common::NS_IN_DAY;

/// A notification request the scripted peer answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    /// A registration of the installation.
    Register,
    /// A deletion of the registration.
    Unregister,
    /// A batch of subscription adds and removes.
    Update,
    /// A publish of the recipient secret.
    Publish,
}

/// The registration state of the scripted peer and the faults it injects.
#[derive(Default)]
pub struct Server {
    /// Whether the installation is registered.
    pub registered: bool,
    /// The delivery of the last registration.
    pub delivery: Option<wire::register_request::Delivery>,
    /// The adds and removes of each subscription update, in order.
    pub batches: Vec<(usize, usize)>,
    /// The subscriptions by topic.
    pub subscriptions: BTreeMap<Vec<u8>, wire::Subscription>,
    /// Topics the recipient state counts beyond `subscriptions`.
    pub extra: u64,
    /// Refuse an update that leaves more subscriptions than this.
    pub limit: Option<usize>,
    /// Every request, in order.
    pub calls: Vec<Call>,
    /// Hold the next request until `Peer::release`.
    pub pause_next: bool,
    /// Apply the next update but never answer it.
    pub lose_update_reply: bool,
    /// Refuse the next request with this code.
    pub next_error: Option<tonic::Code>,
    /// Refuse the next request of this kind with a rate limit of this delay.
    pub rate_limit: Option<(Call, xmtp_common::time::Duration)>,
    /// Answer the next request of this kind only after this long.
    pub slow: Option<(Call, xmtp_common::time::Duration)>,
}

/// The scripted notification backend that a [`ScriptedApi`] forwards to.
#[derive(Default)]
pub struct Peer {
    /// The registration state and the faults to inject.
    pub state: Mutex<Server>,
    /// Notified when a held request or a lost update reply arrives.
    pub entered: Notify,
    /// Releases a held request.
    pub release: Notify,
}

fn status(code: tonic::Code) -> ApiClientError {
    ApiClientError::OtherUnretryable(Box::new(tonic::Status::new(
        code,
        "notification test response",
    )))
}

impl Peer {
    async fn gate(&self, call: Call) -> Result<(), ApiClientError> {
        let (pause, error, rate_limit, slow) = {
            let mut state = self.state.lock();
            state.calls.push(call);
            let pause = std::mem::take(&mut state.pause_next);
            let rate_limit = state
                .rate_limit
                .take_if(|(kind, _)| *kind == call)
                .map(|(_, delay)| delay);
            let slow = state
                .slow
                .take_if(|(kind, _)| *kind == call)
                .map(|(_, delay)| delay);
            (pause, state.next_error.take(), rate_limit, slow)
        };
        if pause {
            self.entered.notify_one();
            self.release.notified().await;
        }
        if let Some(delay) = slow {
            xmtp_common::time::sleep(delay).await;
        }
        if let Some(delay) = rate_limit {
            return Err(ApiClientError::client(
                xmtp_api_grpc::error::GrpcError::Status(xmtp_api_grpc::error::rate_limited_status(
                    "push", delay,
                )),
            ));
        }
        error.map(status).map_or(Ok(()), Err)
    }

    /// The recipient state the peer confirms: its topic count and channel,
    /// expiring [`TEST_TTL_NS`] from now.
    pub fn response(&self) -> wire::RecipientState {
        let state = self.state.lock();
        wire::RecipientState {
            topic_count: state.subscriptions.len() as u64 + state.extra,
            channel: match state.delivery {
                Some(wire::register_request::Delivery::Apns(_)) => wire::Channel::Apns,
                Some(wire::register_request::Delivery::Fcm(_)) => wire::Channel::Fcm,
                _ => wire::Channel::Http,
            } as i32,
            expires_at_ns: xmtp_common::time::now_ns() + TEST_TTL_NS,
        }
    }

    /// The number of `call` requests the peer has received.
    pub fn calls(&self, call: Call) -> usize {
        self.state
            .lock()
            .calls
            .iter()
            .filter(|seen| **seen == call)
            .count()
    }
}

/// A peer backend client whose notification requests the scripted [`Peer`]
/// answers; every other request reaches the real backend.
#[derive(Clone)]
pub struct ScriptedApi {
    inner: PeerApi,
    /// The scripted notification backend.
    pub peer: Arc<Peer>,
}

impl ScriptedApi {
    /// The backend client `inner`, with `peer` answering its notification
    /// requests.
    pub fn new(inner: PeerApi, peer: Arc<Peer>) -> Self {
        Self { inner, peer }
    }
}

#[xmtp_common::async_trait]
impl XmtpBackendClient for ScriptedApi {
    type Error = ApiClientError;

    async fn publish(
        &self,
        request: wire::PublishRequest,
    ) -> Result<wire::PublishResponse, Self::Error> {
        self.peer.state.lock().calls.push(Call::Publish);
        self.inner.publish(request).await
    }
    async fn create_upload(
        &self,
        request: wire::CreateUploadRequest,
    ) -> Result<wire::CreateUploadResponse, Self::Error> {
        self.inner.create_upload(request).await
    }
    async fn query(&self, request: wire::QueryRequest) -> Result<wire::QueryResponse, Self::Error> {
        self.inner.query(request).await
    }
    async fn query_raw_envelopes(
        &self,
        request: wire::QueryRequest,
    ) -> Result<xmtp_proto::types::ReceivedQueryResponse, Self::Error> {
        self.query(request).await.map(Into::into)
    }
    async fn query_newest(
        &self,
        request: wire::QueryNewestRequest,
    ) -> Result<wire::QueryNewestResponse, Self::Error> {
        self.inner.query_newest(request).await
    }
    async fn get_inbox_ids(
        &self,
        request: wire::GetInboxIdsRequest,
    ) -> Result<wire::GetInboxIdsResponse, Self::Error> {
        self.inner.get_inbox_ids(request).await
    }
    async fn get_configuration(
        &self,
        request: wire::GetConfigurationRequest,
    ) -> Result<wire::GetConfigurationResponse, Self::Error> {
        self.inner.get_configuration(request).await
    }
    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: wire::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<wire::VerifySmartContractWalletSignaturesResponse, Self::Error> {
        self.inner
            .verify_smart_contract_wallet_signatures(request)
            .await
    }

    async fn register(
        &self,
        mut request: wire::RegisterRequest,
    ) -> Result<wire::RecipientState, Self::Error> {
        self.peer.gate(Call::Register).await?;
        {
            let mut state = self.peer.state.lock();
            state.registered = true;
            state.delivery = request.delivery.take();
        }
        Ok(self.peer.response())
    }

    async fn unregister(
        &self,
        _: wire::UnregisterRequest,
    ) -> Result<wire::UnregisterResponse, Self::Error> {
        self.peer.gate(Call::Unregister).await?;
        let mut state = self.peer.state.lock();
        if !state.registered {
            return Err(status(tonic::Code::NotFound));
        }
        state.registered = false;
        state.subscriptions.clear();
        Ok(wire::UnregisterResponse {})
    }

    async fn update_subscriptions(
        &self,
        mut request: wire::UpdateSubscriptionsRequest,
    ) -> Result<wire::RecipientState, Self::Error> {
        self.peer.gate(Call::Update).await?;
        {
            let mut state = self.peer.state.lock();
            state
                .batches
                .push((request.adds.len(), request.removes.len()));
            if !state.registered {
                return Err(status(tonic::Code::NotFound));
            }
            let mut next = state.subscriptions.clone();
            for topic in std::mem::take(&mut request.removes) {
                next.remove(&topic);
            }
            for subscription in std::mem::take(&mut request.adds) {
                next.insert(subscription.topic.clone(), subscription);
            }
            if state.limit.is_some_and(|limit| next.len() > limit) {
                return Err(status(tonic::Code::ResourceExhausted));
            }
            state.subscriptions = next;
        }
        let lose_reply = std::mem::take(&mut self.peer.state.lock().lose_update_reply);
        if lose_reply {
            self.peer.entered.notify_one();
            std::future::pending::<()>().await;
        }
        Ok(self.peer.response())
    }
}

impl PeerFeatures<ScriptedApi> for EngineTestFeatures {
    /// The features of a client build over `ClientBuilder::api_client`,
    /// which opens no streams.
    fn new(
        _: &ApiClientWrapper<ScriptedApi>,
        policy: &StreamPolicy,
        own_inbox_stored_through: Option<i64>,
    ) -> Self {
        Self::query_only(policy, own_inbox_stored_through)
    }
}

/// A registered peer whose notification requests `Peer` answers.
pub type ScriptedPeer = EngineTestPeer<ScriptedApi>;

/// A registered installation that reopens over a `ScriptedApi`, as a test
/// client rebuilt with `ClientBuilder::api_client` and its workers stopped
/// opens: offline, from its stored state, with no streams.
pub async fn peer() -> (Arc<ScriptedPeer>, Arc<Peer>) {
    let (client, peer) = scripted(&EngineTestPeer::new().await, Arc::new(Peer::default())).await;
    (Arc::new(client), peer)
}

/// `installation` reopened over a `ScriptedApi` whose notification requests
/// `peer` answers.
async fn scripted(installation: &EngineTestPeer, peer: Arc<Peer>) -> (ScriptedPeer, Arc<Peer>) {
    let api = ScriptedApi::new(
        installation.context.api().api_client.raw_for_test().clone(),
        peer.clone(),
    );
    (installation.with_api(api, None, |_| {}).await, peer)
}

/// `client`'s installation opened from `snapshot` over the test backend,
/// as a test client built from a snapshot with its workers disabled opens.
pub async fn restored(client: &ScriptedPeer, snapshot: &[u8]) -> EngineTestPeer {
    let installation: EngineTestPeer = client
        .with_api(
            client.context.api().api_client.raw_for_test().inner.clone(),
            None,
            |_| {},
        )
        .await;
    installation.from_snapshot(snapshot, |_| {}).await
}

/// `client`'s installation opened from `snapshot` with a fresh task runner
/// and the same remote `peer`.
pub async fn client_from_snapshot(
    client: &ScriptedPeer,
    snapshot: &[u8],
    peer: Arc<Peer>,
) -> Arc<ScriptedPeer> {
    Arc::new(scripted(&restored(client, snapshot).await, peer).await.0)
}

/// The Client's notification forwards, on a peer's context.
#[allow(async_fn_in_trait)]
pub trait NotificationPeer {
    /// The peer's database connection.
    fn db(&self) -> DefaultDbConnection;
    /// As `Client::enable_notifications`.
    async fn enable_notifications(
        &self,
        config: NotificationConfig,
    ) -> Result<NotificationState, NotificationError>;
    /// As `Client::disable_notifications`.
    async fn disable_notifications(&self) -> Result<(), NotificationError>;
    /// As `Client::notification_state`.
    fn notification_state(&self) -> Result<NotificationState, xmtp_db::StorageError>;
    /// A copy of the peer's database, as a test client's `db_snapshot`.
    fn db_snapshot(&self) -> Vec<u8>;
}

impl<A: XmtpApi + 'static> NotificationPeer for TestPeer<EngineTestFeatures, A> {
    fn db(&self) -> DefaultDbConnection {
        self.context.db()
    }

    async fn enable_notifications(
        &self,
        config: NotificationConfig,
    ) -> Result<NotificationState, NotificationError> {
        super::enable_notifications(&self.context, config).await
    }

    async fn disable_notifications(&self) -> Result<(), NotificationError> {
        super::disable_notifications(&self.context).await
    }

    fn notification_state(&self) -> Result<NotificationState, xmtp_db::StorageError> {
        super::notification_state(&self.context)
    }

    fn db_snapshot(&self) -> Vec<u8> {
        use xmtp_db::ConnectionExt;
        self.db()
            .raw_query(|conn| Ok(conn.serialize_database_to_buffer().to_vec()))
            .expect("a database snapshot")
    }
}
