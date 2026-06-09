//! XIP-83 bidirectional subscription client (native-only).
//!
//! [`BidiSubscription`] owns one long-lived bidi `Subscribe` stream: the topic
//! set is mutated in place via [`BidiSubscription::mutate`] (no reconnect on
//! membership change) and liveness is WebSocket-style ping/pong — the actor
//! answers server `Ping`s automatically and the consumer can probe a resumed
//! link with [`BidiSubscription::ping`].
//!
//! Concurrency model mirrors the server's single writer: one spawned task owns
//! the inbound wire stream and is the only producer of [`BidiEvent`]s, so the
//! wire order (history, then the topic's `TopicsLive` marker, then live tail;
//! a wave's frames before its `CATCHUP_COMPLETE`) is preserved verbatim in the
//! event order. Outbound frames flow over one channel, so client frames are
//! FIFO too. Reconnect policy (resume from durable cursors) belongs to the
//! caller: when the stream dies, [`BidiSubscription::next`] returns `None` and
//! the consumer re-opens from its persisted cursors.

use futures::StreamExt;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use xmtp_common::{AbortHandle, StreamHandle};
use xmtp_proto::api_client::XmtpMlsBidiStreams;
use xmtp_proto::mls_v1::{
    GroupMessage, Ping, Pong, SubscribeRequest, SubscribeResponse, WelcomeMessage,
    subscribe_request, subscribe_response,
};

pub use xmtp_proto::mls_v1::subscribe_request::v1::{Mutate, mutate::Subscription};
pub use xmtp_proto::types::{Topic, TopicKind};

/// Outbound frames awaiting the transport. Mutates are small and pongs must
/// not queue behind much, so this stays shallow.
const OUTBOUND_BUFFER: usize = 64;
/// Inbound events awaiting the consumer. When this fills, the actor stops
/// reading the wire; a consumer stalled past the server's pong deadline is
/// reaped by the node ("consumer too slow"), which is the intended liveness
/// semantics rather than unbounded buffering.
const EVENT_BUFFER: usize = 1024;

/// A server→client occurrence on the subscription, in exact wire order.
#[derive(Debug, Clone, PartialEq)]
pub enum BidiEvent {
    /// First frame on the stream; advertises the server's ping cadence (the
    /// basis for the client's staleness threshold) and the optional protocol
    /// features this node supports. Raw enum values so a newer server's
    /// capabilities survive an older client (send nothing the node did not
    /// advertise).
    Started {
        keepalive_interval_ms: u32,
        capabilities: Vec<i32>,
    },
    /// A Mutate's added subscriptions have all caught up to live (one per
    /// adding Mutate); `mutate_id` echoes the Mutate that started the wave,
    /// so completions stay attributable when waves overlap.
    CatchUpComplete { mutate_id: u64 },
    /// These kind-prefixed topics just crossed from catch-up to live: every
    /// later frame for a listed topic is live tail (e.g. notify for it,
    /// don't backfill it).
    TopicsLive { topics: Vec<Vec<u8>> },
    GroupMessages(Vec<GroupMessage>),
    WelcomeMessages(Vec<WelcomeMessage>),
    /// Answer to a client-initiated [`BidiSubscription::ping`].
    Pong { nonce: u64 },
}

#[derive(Debug, thiserror::Error)]
pub enum BidiSubscriptionError {
    /// The stream is gone; re-open and resume from durable cursors.
    #[error("the bidirectional subscription has closed")]
    Closed,
}

/// One open bidirectional subscription stream.
pub struct BidiSubscription {
    outbound: mpsc::Sender<SubscribeRequest>,
    events: mpsc::Receiver<BidiEvent>,
    ping_nonce: AtomicU64,
    actor: Box<dyn AbortHandle>,
}

impl BidiSubscription {
    /// Open the stream and send `initial` as the first Mutate (it names the
    /// initial topic set with per-topic resume cursors; see XIP-83 client
    /// requirement 3 for cursor discipline).
    pub async fn open<A>(api: &A, initial: Mutate) -> Result<Self, A::Error>
    where
        A: XmtpMlsBidiStreams,
        A::SubscribeStream: 'static,
    {
        let (outbound, outbound_rx) = mpsc::channel(OUTBOUND_BUFFER);
        let (event_tx, events) = mpsc::channel(EVENT_BUFFER);

        let inbound = api
            .subscribe_bidi(Box::pin(ReceiverStream::new(outbound_rx)))
            .await?;

        let actor = xmtp_common::spawn(
            None,
            run_actor(Box::pin(inbound), outbound.clone(), event_tx),
        );

        let this = Self {
            outbound,
            events,
            ping_nonce: AtomicU64::new(0),
            actor: actor.abort_handle(),
        };
        // Buffer capacity is fresh, so this cannot block meaningfully.
        this.mutate(initial).await.map_err(|_| ()).ok();
        Ok(this)
    }

    /// Add/remove subscriptions in place, without reconnecting.
    pub async fn mutate(&self, mutate: Mutate) -> Result<(), BidiSubscriptionError> {
        self.send(subscribe_request::v1::Request::Mutate(mutate))
            .await
    }

    /// Probe the link (e.g. right after the process resumes): the server MUST
    /// answer with a [`BidiEvent::Pong`] echoing the returned nonce. No pong
    /// (or a send failure) means the stream is dead — re-open from cursors.
    pub async fn ping(&self) -> Result<u64, BidiSubscriptionError> {
        let nonce = self.ping_nonce.fetch_add(1, Ordering::Relaxed) + 1;
        self.send(subscribe_request::v1::Request::Ping(Ping { nonce }))
            .await?;
        Ok(nonce)
    }

    /// Next event, in wire order. `None` means the stream ended (server close,
    /// network death, or reap) — resume from durable cursors on a new stream.
    pub async fn next(&mut self) -> Option<BidiEvent> {
        self.events.recv().await
    }

    async fn send(
        &self,
        request: subscribe_request::v1::Request,
    ) -> Result<(), BidiSubscriptionError> {
        self.outbound
            .send(request_frame(request))
            .await
            .map_err(|_| BidiSubscriptionError::Closed)
    }
}

impl Drop for BidiSubscription {
    fn drop(&mut self) {
        // Stop the actor so it cannot keep auto-ponging (a zombie keepalive
        // would hold the server-side subscription open forever). Dropping the
        // actor's inbound stream cancels the underlying request.
        self.actor.end();
    }
}

fn request_frame(request: subscribe_request::v1::Request) -> SubscribeRequest {
    SubscribeRequest {
        version: Some(subscribe_request::Version::V1(subscribe_request::V1 {
            request: Some(request),
        })),
    }
}

/// The single writer: sole reader of the wire, sole producer of events. Ends
/// when the wire ends/errors or the consumer goes away; ending drops its
/// half of the channels, which is the close signal in both directions.
async fn run_actor<S, E>(
    mut inbound: S,
    outbound: mpsc::Sender<SubscribeRequest>,
    events: mpsc::Sender<BidiEvent>,
) where
    S: futures::Stream<Item = Result<SubscribeResponse, E>> + Unpin,
    E: std::fmt::Display,
{
    while let Some(frame) = inbound.next().await {
        let response = match frame {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("bidi subscription stream errored: {e}");
                break;
            }
        };
        let Some(subscribe_response::Version::V1(v1)) = response.version else {
            // A version we did not speak; XIP-83 pins responses to the request
            // version, so this is a server bug — skip it rather than die.
            tracing::warn!("bidi subscription received unknown response version");
            continue;
        };
        use subscribe_response::v1::Response;
        match v1.response {
            Some(Response::Ping(ping)) => {
                // Liveness: answer immediately (FIFO with any queued frames).
                let pong = request_frame(subscribe_request::v1::Request::Pong(Pong {
                    nonce: ping.nonce,
                }));
                if outbound.send(pong).await.is_err() {
                    break;
                }
            }
            Some(Response::Pong(pong)) => {
                if emit(&events, BidiEvent::Pong { nonce: pong.nonce }).await {
                    break;
                }
            }
            Some(Response::Started(started)) => {
                let event = BidiEvent::Started {
                    keepalive_interval_ms: started.keepalive_interval_ms,
                    capabilities: started.capabilities,
                };
                if emit(&events, event).await {
                    break;
                }
            }
            Some(Response::CatchupComplete(complete)) => {
                let event = BidiEvent::CatchUpComplete {
                    mutate_id: complete.mutate_id,
                };
                if emit(&events, event).await {
                    break;
                }
            }
            Some(Response::TopicsLive(live)) => {
                let event = BidiEvent::TopicsLive {
                    topics: live.topics,
                };
                if emit(&events, event).await {
                    break;
                }
            }
            Some(Response::Messages(messages)) => {
                if !messages.group_messages.is_empty()
                    && emit(&events, BidiEvent::GroupMessages(messages.group_messages)).await
                {
                    break;
                }
                if !messages.welcome_messages.is_empty()
                    && emit(
                        &events,
                        BidiEvent::WelcomeMessages(messages.welcome_messages),
                    )
                    .await
                {
                    break;
                }
            }
            // An arm added by a future server revision: informational frames
            // are safe to skip (delivery correctness never depends on them).
            None => continue,
        }
    }
}

/// Send an event to the consumer; returns true when the consumer is gone and
/// the actor should shut down.
async fn emit(events: &mpsc::Sender<BidiEvent>, event: BidiEvent) -> bool {
    events.send(event).await.is_err()
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::BoxStream;
    use std::sync::Mutex;
    use xmtp_proto::api::ApiClientError;
    use xmtp_proto::mls_v1::{group_message, welcome_message};

    /// A scripted peer: captures every frame the client sends and lets the
    /// test play server frames into the subscription.
    struct MockBidiApi {
        inbound: Mutex<Option<mpsc::UnboundedReceiver<Result<SubscribeResponse, ApiClientError>>>>,
        captured: mpsc::UnboundedSender<SubscribeRequest>,
    }

    struct MockServer {
        to_client: mpsc::UnboundedSender<Result<SubscribeResponse, ApiClientError>>,
        from_client: mpsc::UnboundedReceiver<SubscribeRequest>,
    }

    fn mock_pair() -> (MockBidiApi, MockServer) {
        let (to_client, inbound) = mpsc::unbounded_channel();
        let (captured, from_client) = mpsc::unbounded_channel();
        (
            MockBidiApi {
                inbound: Mutex::new(Some(inbound)),
                captured,
            },
            MockServer {
                to_client,
                from_client,
            },
        )
    }

    #[xmtp_common::async_trait]
    impl XmtpMlsBidiStreams for MockBidiApi {
        type SubscribeStream = BoxStream<'static, Result<SubscribeResponse, ApiClientError>>;
        type Error = ApiClientError;

        async fn subscribe_bidi(
            &self,
            requests: BoxStream<'static, SubscribeRequest>,
        ) -> Result<Self::SubscribeStream, Self::Error> {
            let captured = self.captured.clone();
            xmtp_common::spawn(None, async move {
                let mut requests = requests;
                while let Some(frame) = requests.next().await {
                    let _ = captured.send(frame);
                }
            });
            let inbound = self
                .inbound
                .lock()
                .unwrap()
                .take()
                .expect("subscribe_bidi called twice");
            Ok(Box::pin(
                tokio_stream::wrappers::UnboundedReceiverStream::new(inbound),
            ))
        }
    }

    impl MockServer {
        fn send(&self, response: subscribe_response::v1::Response) {
            self.to_client
                .send(Ok(SubscribeResponse {
                    version: Some(subscribe_response::Version::V1(subscribe_response::V1 {
                        response: Some(response),
                    })),
                }))
                .unwrap();
        }

        async fn next_request(&mut self) -> subscribe_request::v1::Request {
            let frame = self.from_client.recv().await.expect("client closed");
            let Some(subscribe_request::Version::V1(v1)) = frame.version else {
                panic!("client sent unknown request version");
            };
            v1.request.expect("client sent empty request")
        }
    }

    fn started(keepalive: u32, capabilities: Vec<i32>) -> subscribe_response::v1::Response {
        subscribe_response::v1::Response::Started(subscribe_response::v1::Started {
            keepalive_interval_ms: keepalive,
            capabilities,
        })
    }

    fn catchup_complete(mutate_id: u64) -> subscribe_response::v1::Response {
        subscribe_response::v1::Response::CatchupComplete(
            subscribe_response::v1::CatchupComplete { mutate_id },
        )
    }

    fn wire_topic(kind: TopicKind, identifier: &[u8]) -> Vec<u8> {
        let mut topic = vec![kind as u8];
        topic.extend_from_slice(identifier);
        topic
    }

    fn group_msg(id: u64, data: &[u8]) -> GroupMessage {
        GroupMessage {
            version: Some(group_message::Version::V1(group_message::V1 {
                id,
                created_ns: id,
                group_id: b"group".to_vec(),
                data: data.to_vec(),
                sender_hmac: vec![],
                should_push: false,
                is_commit: false,
            })),
        }
    }

    fn welcome_msg(id: u64, installation_key: &[u8]) -> WelcomeMessage {
        WelcomeMessage {
            version: Some(welcome_message::Version::V1(welcome_message::V1 {
                id,
                created_ns: id,
                installation_key: installation_key.to_vec(),
                data: b"welcome".to_vec(),
                hpke_public_key: vec![],
                wrapper_algorithm: 0,
                welcome_metadata: vec![],
            })),
        }
    }

    fn initial_mutate() -> Mutate {
        Mutate {
            adds: vec![
                Subscription {
                    topic: wire_topic(TopicKind::GroupMessagesV1, b"group"),
                    id_cursor: 5,
                },
                Subscription {
                    topic: wire_topic(TopicKind::WelcomeMessagesV1, b"installation"),
                    id_cursor: 0,
                },
            ],
            ..Default::default()
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn bidi_sends_initial_mutate_and_emits_started() {
        let (api, mut server) = mock_pair();
        let mut sub = BidiSubscription::open(&api, initial_mutate()).await?;

        let subscribe_request::v1::Request::Mutate(sent) = server.next_request().await else {
            panic!("first frame must be the initial Mutate");
        };
        assert_eq!(sent, initial_mutate());

        // A capability value from a future server revision survives verbatim.
        server.send(started(30_000, vec![7]));
        assert_eq!(
            sub.next().await,
            Some(BidiEvent::Started {
                keepalive_interval_ms: 30_000,
                capabilities: vec![7],
            })
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn bidi_auto_pongs_server_ping() {
        let (api, mut server) = mock_pair();
        let _sub = BidiSubscription::open(&api, Mutate::default()).await?;
        server.next_request().await; // initial mutate

        server.send(subscribe_response::v1::Response::Ping(Ping { nonce: 42 }));

        let subscribe_request::v1::Request::Pong(pong) = server.next_request().await else {
            panic!("server ping must be answered with a pong");
        };
        assert_eq!(pong.nonce, 42);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn bidi_preserves_wire_order_of_history_markers_and_live() {
        let (api, mut server) = mock_pair();
        let mut sub = BidiSubscription::open(&api, initial_mutate()).await?;
        server.next_request().await;

        let live_topics = vec![
            wire_topic(TopicKind::GroupMessagesV1, b"group"),
            wire_topic(TopicKind::WelcomeMessagesV1, b"installation"),
        ];

        // history → gate flush → TopicsLive → wave CatchupComplete → live
        server.send(subscribe_response::v1::Response::Messages(
            subscribe_response::v1::Messages {
                group_messages: vec![group_msg(6, b"hist")],
                welcome_messages: vec![welcome_msg(1, b"installation")],
            },
        ));
        server.send(subscribe_response::v1::Response::TopicsLive(
            subscribe_response::v1::TopicsLive {
                topics: live_topics.clone(),
            },
        ));
        server.send(catchup_complete(11));
        server.send(subscribe_response::v1::Response::Messages(
            subscribe_response::v1::Messages {
                group_messages: vec![group_msg(7, b"live")],
                welcome_messages: vec![],
            },
        ));

        assert_eq!(
            sub.next().await,
            Some(BidiEvent::GroupMessages(vec![group_msg(6, b"hist")]))
        );
        assert_eq!(
            sub.next().await,
            Some(BidiEvent::WelcomeMessages(vec![welcome_msg(
                1,
                b"installation"
            )]))
        );
        assert_eq!(
            sub.next().await,
            Some(BidiEvent::TopicsLive {
                topics: live_topics
            })
        );
        assert_eq!(
            sub.next().await,
            Some(BidiEvent::CatchUpComplete { mutate_id: 11 })
        );
        assert_eq!(
            sub.next().await,
            Some(BidiEvent::GroupMessages(vec![group_msg(7, b"live")]))
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn bidi_mutates_in_place_and_round_trips_client_ping() {
        let (api, mut server) = mock_pair();
        let mut sub = BidiSubscription::open(&api, Mutate::default()).await?;
        server.next_request().await;

        let joined = wire_topic(TopicKind::GroupMessagesV1, b"joined");
        let left = wire_topic(TopicKind::GroupMessagesV1, b"left");
        sub.mutate(Mutate {
            adds: vec![Subscription {
                topic: joined.clone(),
                id_cursor: 9,
            }],
            removes: vec![left.clone()],
            mutate_id: 3,
            ..Default::default()
        })
        .await?;
        let nonce = sub.ping().await?;

        let subscribe_request::v1::Request::Mutate(mutate) = server.next_request().await else {
            panic!("expected the in-place mutate");
        };
        assert_eq!(mutate.adds[0].topic, joined);
        assert_eq!(mutate.adds[0].id_cursor, 9);
        assert_eq!(mutate.removes, vec![left]);
        assert_eq!(mutate.mutate_id, 3);

        let subscribe_request::v1::Request::Ping(ping) = server.next_request().await else {
            panic!("expected the client ping");
        };
        assert_eq!(ping.nonce, nonce);

        server.send(subscribe_response::v1::Response::Pong(Pong { nonce }));
        assert_eq!(sub.next().await, Some(BidiEvent::Pong { nonce }));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn bidi_skips_unknown_frames_and_survives() {
        let (api, mut server) = mock_pair();
        let mut sub = BidiSubscription::open(&api, Mutate::default()).await?;
        server.next_request().await;

        // A response version we did not speak, and an empty V1 arm (a frame
        // type from a future revision): both skipped, neither fatal.
        server.to_client.send(Ok(SubscribeResponse { version: None }))?;
        server
            .to_client
            .send(Ok(SubscribeResponse {
                version: Some(subscribe_response::Version::V1(subscribe_response::V1 {
                    response: None,
                })),
            }))?;
        server.send(started(1_000, vec![]));

        assert_eq!(
            sub.next().await,
            Some(BidiEvent::Started {
                keepalive_interval_ms: 1_000,
                capabilities: vec![],
            })
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn bidi_stream_end_closes_events_and_sends() {
        let (api, mut server) = mock_pair();
        let mut sub = BidiSubscription::open(&api, Mutate::default()).await?;
        server.next_request().await;

        drop(server.to_client);

        assert_eq!(sub.next().await, None, "stream end must surface as None");
    }
}
