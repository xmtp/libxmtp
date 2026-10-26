use super::*;
use crate::context::XmtpMlsLocalContext;
use crate::identity::{Identity, IdentityStrategy};
use futures::StreamExt;
use prost::Message;
use std::sync::atomic::{AtomicUsize, Ordering};
use xmtp_api_backend::BackendClient;
use xmtp_common::{
    BoxDynStream,
    time::{Instant, timeout},
};
use xmtp_configuration::{ConfigProvider, StaticConfigProvider};
use xmtp_db::XmtpTestDb;
use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
use xmtp_proto::{
    api::{ApiClientError, BytesStream, Client as WireClient, NetworkError},
    backend_v1::{
        CatchupTarget, SubscribeRequest, SubscribeResponse, subscribe_request, subscribe_response,
    },
    types::{Cursor, IncomingBatchLimits, IncomingSubscription, Topic, TopicCursor},
};

struct Arrival {
    wire: usize,
    at: Instant,
}

struct ScriptedPeer {
    updates: tokio::sync::mpsc::UnboundedSender<Arrival>,
    opens: AtomicUsize,
}

#[xmtp_common::async_trait]
impl WireClient for ScriptedPeer {
    fn host(&self) -> &str {
        "test://admitted-snapshot-update-pacing"
    }

    async fn request(
        &self,
        _request: http::request::Builder,
        _path: http::uri::PathAndQuery,
        _body: prost::bytes::Bytes,
    ) -> Result<http::Response<prost::bytes::Bytes>, ApiClientError> {
        panic!("offline snapshot fixture must not make unary requests");
    }

    async fn stream(
        &self,
        _request: http::request::Builder,
        _path: http::uri::PathAndQuery,
        _body: prost::bytes::Bytes,
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        panic!("native snapshot fixture must use bidi");
    }

    async fn bidi_stream(
        &self,
        _request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: BoxDynStream<'static, prost::bytes::Bytes>,
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        assert_eq!(
            path.path(),
            "/xmtp.backend.v1.SubscriptionService/Subscribe"
        );
        let wire = self.opens.fetch_add(1, Ordering::SeqCst);
        let updates = self.updates.clone();
        let applied = body.filter_map(move |bytes| {
            let updates = updates.clone();
            async move {
                let frame = SubscribeRequest::decode(bytes).expect("SubscribeRequest");
                let Some(subscribe_request::Request::Update(update)) = frame.request else {
                    return None;
                };
                let _ = updates.send(Arrival {
                    wire,
                    at: Instant::now(),
                });
                Some(Ok(prost::bytes::Bytes::from(
                    SubscribeResponse {
                        response: Some(subscribe_response::Response::Applied(
                            subscribe_response::Applied {
                                id: update.id,
                                added_targets: update
                                    .adds
                                    .into_iter()
                                    .map(|add| CatchupTarget {
                                        topic: add.topic,
                                        through_sequence_id: 0,
                                    })
                                    .collect(),
                            },
                        )),
                    }
                    .encode_to_vec(),
                )))
            }
        });
        let started = futures::stream::once(async {
            Ok(prost::bytes::Bytes::from(
                SubscribeResponse {
                    response: Some(subscribe_response::Response::Started(
                        subscribe_response::Started {
                            keepalive_interval_ms: 30_000,
                        },
                    )),
                }
                .encode_to_vec(),
            ))
        });
        Ok(http::Response::new(BytesStream::new(
            started.chain(applied),
        )))
    }
}

type Api = Arc<BackendClient<Arc<ScriptedPeer>>>;
type Context = Arc<XmtpMlsLocalContext<Api, xmtp_db::DefaultStore, xmtp_db::DefaultMlsStore>>;

async fn client_with_snapshot(api: Api, strict: bool) -> Client<Context> {
    let provider: Arc<dyn ConfigProvider> =
        Arc::new(StaticConfigProvider::edited(move |configuration| {
            configuration.limits.max_update_frames_per_second = if strict { 1 } else { 10 };
            configuration.limits.max_update_burst = if strict { 1 } else { 100 };
        }));
    Client::builder(IdentityStrategy::ExternalIdentity(Identity::mock_identity()))
        .api_client_with_streams(api)
        .store(xmtp_db::TestDb::create_ephemeral_store().await)
        .with_allow_offline(Some(true))
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .with_disable_workers(true)
        .config_provider(provider)
        .default_mls_store()
        .expect("MLS store")
        .build()
        .await
        .expect("client with admitted snapshot")
}

async fn open(client: &Client<Context>, topic: u8) -> IncomingSubscription<NetworkError> {
    let cursors: TopicCursor = [(Topic::new_group_message([topic; 16]), Cursor(0))].into();
    let limits = IncomingBatchLimits {
        max_rows: 8,
        max_bytes: 1024,
    };
    client
        .context
        .incoming_runtime
        .factory
        .as_ref()
        .expect("production factory")
        .open(cursors, limits)
        .await
        .expect("open production subscription")
}

async fn next_update(observed: &mut tokio::sync::mpsc::UnboundedReceiver<Arrival>) -> Arrival {
    timeout(WAIT, observed.recv())
        .await
        .expect("Update timeout")
        .expect("observed Update")
}

// verifies: CONF-093
#[rstest::rstest]
#[case::active_loose_wire(false)]
#[case::strict_before_later_loose_admission(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn shared_transport_paces_updates_to_the_client_snapshot(#[case] strict_first: bool) {
    let (updates, mut observed) = tokio::sync::mpsc::unbounded_channel();
    let api = Arc::new(BackendClient::new(Arc::new(ScriptedPeer {
        updates,
        opens: AtomicUsize::new(0),
    })));
    let (strict, loose, loose_subscription, loose_opening) = if strict_first {
        let strict = client_with_snapshot(api.clone(), true).await;
        let loose = client_with_snapshot(api, false).await;
        let subscription = open(&loose, 7).await;
        let opening = next_update(&mut observed).await;
        (strict, loose, subscription, opening)
    } else {
        let loose = client_with_snapshot(api.clone(), false).await;
        let subscription = open(&loose, 7).await;
        let opening = next_update(&mut observed).await;
        let strict = client_with_snapshot(api, true).await;
        (strict, loose, subscription, opening)
    };
    assert_eq!(strict.server_configuration().limits.max_update_burst, 1);
    assert_eq!(loose.server_configuration().limits.max_update_burst, 100);

    let first = open(&strict, 8).await;
    let opening = next_update(&mut observed).await;
    let second = open(&strict, 9).await;
    let next = next_update(&mut observed).await;
    assert!(next.at.duration_since(opening.at) >= Duration::from_millis(750));
    assert_ne!(
        opening.wire, loose_opening.wire,
        "different snapshots need separate ledgers"
    );
    assert_eq!(
        next.wire, opening.wire,
        "one client keeps its own shared wire"
    );
    drop((first, second, loose_subscription));
    strict.close().await.expect("close strict client");
    loose.close().await.expect("close loose client");
}
