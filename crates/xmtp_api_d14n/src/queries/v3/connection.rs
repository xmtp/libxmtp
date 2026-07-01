//! v3 (MLS API) binding for the XIP-83 bidirectional subscription connection.
//!
//! The control core lives in [`crate::queries::bidi`]; this supplies the v3 wire
//! vocabulary (`mls_v1` frames) and surfaces messages as the raw proto
//! `GroupMessage`/`WelcomeMessage` (v3 carries a single id cursor and the
//! consumer decodes), via the [`BidiBinding`] trait. Native-only.

use crate::queries::bidi::{BidiBinding, Connection, Event, Inbound, parse_topics};
use xmtp_proto::api_client::XmtpMlsBidiStreams;
use xmtp_proto::mls_v1::subscribe_request::v1::Mutate;
use xmtp_proto::mls_v1::{
    GroupMessage, Ping, Pong, SubscribeRequest, SubscribeResponse, WelcomeMessage,
    subscribe_request, subscribe_response,
};

/// The v3 (MLS API) wire binding for a bidi subscription.
pub struct V3Binding;

/// A v3 bidirectional subscription connection (XIP-83). See [`Connection`].
pub type BidiConnection = Connection<V3Binding>;
/// Events surfaced by a v3 [`BidiConnection`], in wire order.
pub type BidiEvent = Event<GroupMessage, WelcomeMessage>;

fn request_frame(request: subscribe_request::v1::Request) -> SubscribeRequest {
    SubscribeRequest {
        version: Some(subscribe_request::Version::V1(subscribe_request::V1 {
            request: Some(request),
        })),
    }
}

impl BidiBinding for V3Binding {
    type Request = SubscribeRequest;
    type Response = SubscribeResponse;
    type Mutate = Mutate;
    type GroupMessage = GroupMessage;
    type WelcomeMessage = WelcomeMessage;

    fn mutate_frame(mutate: Mutate) -> SubscribeRequest {
        request_frame(subscribe_request::v1::Request::Mutate(mutate))
    }

    fn ping_frame(nonce: u64) -> SubscribeRequest {
        request_frame(subscribe_request::v1::Request::Ping(Ping { nonce }))
    }

    fn pong_frame(nonce: u64) -> SubscribeRequest {
        request_frame(subscribe_request::v1::Request::Pong(Pong { nonce }))
    }

    fn handle(response: SubscribeResponse) -> Inbound<GroupMessage, WelcomeMessage> {
        let Some(subscribe_response::Version::V1(v1)) = response.version else {
            // A version we did not speak; XIP-83 pins responses to the request
            // version, so this is a server bug — skip, don't die.
            tracing::warn!("bidi subscription received unknown response version");
            return Inbound::Skip;
        };
        use subscribe_response::v1::Response;
        match v1.response {
            // Liveness is internal; the core auto-pongs and correlates probes.
            Some(Response::Ping(ping)) => Inbound::Ping(ping.nonce),
            Some(Response::Pong(pong)) => Inbound::Pong(pong.nonce),
            Some(Response::Started(started)) => Inbound::Emit(Event::Started {
                keepalive_interval_ms: started.keepalive_interval_ms,
                capabilities: started.capabilities,
            }),
            Some(Response::CatchupComplete(complete)) => Inbound::Emit(Event::CatchUpComplete {
                mutate_id: complete.mutate_id,
            }),
            Some(Response::TopicsLive(live)) => Inbound::Emit(Event::TopicsLive {
                topics: parse_topics(live.topics),
            }),
            Some(Response::Messages(messages)) => Inbound::Messages {
                group: messages.group_messages,
                welcome: messages.welcome_messages,
            },
            // A future-revision arm: informational frames are safe to skip.
            None => Inbound::Skip,
        }
    }
}

impl BidiConnection {
    /// Open the stream and send `initial` as the first Mutate (it names the
    /// initial topic set with per-topic resume cursors; XIP-83 client req 3).
    pub async fn open<A>(api: &A, initial: Mutate) -> Result<Self, A::Error>
    where
        A: XmtpMlsBidiStreams,
        A::SubscribeStream: 'static,
    {
        Self::start(initial, |outbound| api.subscribe_bidi(outbound)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::bidi::{
        BidiError, DEFAULT_KEEPALIVE_MS, MAX_PENDING_FRAMES, PROBE_TIMEOUT_MULTIPLIER, WIRE_BUFFER,
    };
    use futures::StreamExt;
    use futures::stream::BoxStream;
    use std::sync::Mutex;
    use std::time::Duration;
    use tokio::sync::mpsc;
    use xmtp_proto::api::ApiClientError;
    use xmtp_proto::mls_v1::subscribe_request::v1::mutate::Subscription;
    use xmtp_proto::mls_v1::{group_message, welcome_message};
    use xmtp_proto::types::{Topic, TopicKind};

    /// A scripted peer: captures every frame the client sends and lets the test
    /// play server frames into the connection.
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
            let mut inbound = self
                .inbound
                .lock()
                .unwrap()
                .take()
                .expect("subscribe_bidi called twice");
            Ok(Box::pin(futures::stream::poll_fn(move |cx| {
                inbound.poll_recv(cx)
            })))
        }
    }

    impl MockServer {
        fn send(&self, response: subscribe_response::v1::Response) {
            self.send_raw(SubscribeResponse {
                version: Some(subscribe_response::Version::V1(subscribe_response::V1 {
                    response: Some(response),
                })),
            });
        }

        fn send_raw(&self, response: SubscribeResponse) {
            self.to_client.send(Ok(response)).unwrap();
        }

        async fn next_request(&mut self) -> subscribe_request::v1::Request {
            let frame = self.from_client.recv().await.expect("client closed");
            let Some(subscribe_request::Version::V1(v1)) = frame.version else {
                panic!("client sent unknown request version");
            };
            v1.request.expect("client sent empty request")
        }
    }

    #[derive(Debug, thiserror::Error)]
    #[error("boom")]
    struct Boom;
    impl xmtp_common::RetryableError for Boom {
        fn is_retryable(&self) -> bool {
            false
        }
    }

    fn started(keepalive: u32, capabilities: Vec<i32>) -> subscribe_response::v1::Response {
        subscribe_response::v1::Response::Started(subscribe_response::v1::Started {
            keepalive_interval_ms: keepalive,
            capabilities,
        })
    }

    fn catchup_complete(mutate_id: u64) -> subscribe_response::v1::Response {
        subscribe_response::v1::Response::CatchupComplete(subscribe_response::v1::CatchupComplete {
            mutate_id,
        })
    }

    fn wire_topic(kind: TopicKind, identifier: &[u8]) -> Vec<u8> {
        // The production constructor, so these tests can't drift from the real
        // kind-prefixed wire layout.
        kind.create(identifier).into()
    }

    fn typed_topic(kind: TopicKind, identifier: &[u8]) -> Topic {
        Topic::try_from(wire_topic(kind, identifier)).unwrap()
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
    async fn open_sends_initial_mutate_and_emits_started() {
        let (api, mut server) = mock_pair();
        let mut conn = BidiConnection::open(&api, initial_mutate()).await?;

        let subscribe_request::v1::Request::Mutate(sent) = server.next_request().await else {
            panic!("first frame must be the initial Mutate");
        };
        assert_eq!(sent, initial_mutate());

        // A capability value from a future server revision survives verbatim.
        server.send(started(30_000, vec![7]));
        assert_eq!(
            conn.next().await,
            Some(BidiEvent::Started {
                keepalive_interval_ms: 30_000,
                capabilities: vec![7],
            })
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn auto_pongs_server_ping_without_surfacing_it() {
        let (api, mut server) = mock_pair();
        let mut conn = BidiConnection::open(&api, Mutate::default()).await?;
        server.next_request().await; // initial mutate

        server.send(subscribe_response::v1::Response::Ping(Ping { nonce: 42 }));
        let subscribe_request::v1::Request::Pong(pong) = server.next_request().await else {
            panic!("server ping must be answered with a pong");
        };
        assert_eq!(pong.nonce, 42);

        // The ping/pong never reaches the consumer: the next event is the Started.
        server.send(started(15_000, vec![]));
        assert_eq!(
            conn.next().await,
            Some(BidiEvent::Started {
                keepalive_interval_ms: 15_000,
                capabilities: vec![],
            })
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn probe_round_trips_and_pong_is_not_an_event() {
        let (api, mut server) = mock_pair();
        let mut conn = BidiConnection::open(&api, Mutate::default()).await?;
        server.next_request().await; // initial mutate

        // `probe` sends a client Ping and awaits its Pong; drive the server side
        // concurrently to answer it.
        let server_side = async {
            let subscribe_request::v1::Request::Ping(ping) = server.next_request().await else {
                panic!("probe must send a Ping");
            };
            server.send(subscribe_response::v1::Response::Pong(Pong {
                nonce: ping.nonce,
            }));
        };
        let (result, ()) = futures::join!(conn.probe(), server_side);
        assert!(matches!(result, Ok(())), "probe should resolve on its pong");

        // The correlating pong was consumed internally, never surfaced.
        server.send(started(10_000, vec![]));
        assert_eq!(
            conn.next().await,
            Some(BidiEvent::Started {
                keepalive_interval_ms: 10_000,
                capabilities: vec![],
            })
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn mutate_is_forwarded_to_the_wire() {
        let (api, mut server) = mock_pair();
        let conn = BidiConnection::open(&api, Mutate::default()).await?;
        server.next_request().await; // initial mutate

        let m = Mutate {
            removes: vec![wire_topic(TopicKind::GroupMessagesV1, b"group")],
            ..Default::default()
        };
        conn.mutate(m.clone()).await?;
        let subscribe_request::v1::Request::Mutate(sent) = server.next_request().await else {
            panic!("mutate must reach the wire");
        };
        assert_eq!(sent, m);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn skips_unknown_version_frames_and_survives() {
        let (api, mut server) = mock_pair();
        let mut conn = BidiConnection::open(&api, Mutate::default()).await?;
        server.next_request().await; // initial mutate

        // A version we don't speak (here: no version at all) is skipped, not fatal.
        server.send_raw(SubscribeResponse { version: None });
        server.send(started(20_000, vec![]));
        assert_eq!(
            conn.next().await,
            Some(BidiEvent::Started {
                keepalive_interval_ms: 20_000,
                capabilities: vec![],
            })
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn preserves_wire_order_of_history_markers_and_live() {
        let (api, mut server) = mock_pair();
        let mut conn = BidiConnection::open(&api, initial_mutate()).await?;
        server.next_request().await;

        let live_topics = vec![
            wire_topic(TopicKind::GroupMessagesV1, b"group"),
            wire_topic(TopicKind::WelcomeMessagesV1, b"installation"),
        ];

        server.send(subscribe_response::v1::Response::Messages(
            subscribe_response::v1::Messages {
                group_messages: vec![group_msg(6, b"hist")],
                welcome_messages: vec![welcome_msg(1, b"installation")],
            },
        ));
        server.send(subscribe_response::v1::Response::TopicsLive(
            subscribe_response::v1::TopicsLive {
                topics: live_topics,
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
            conn.next().await,
            Some(BidiEvent::GroupMessages(vec![group_msg(6, b"hist")]))
        );
        assert_eq!(
            conn.next().await,
            Some(BidiEvent::WelcomeMessages(vec![welcome_msg(
                1,
                b"installation"
            )]))
        );
        assert_eq!(
            conn.next().await,
            Some(BidiEvent::TopicsLive {
                topics: vec![
                    typed_topic(TopicKind::GroupMessagesV1, b"group"),
                    typed_topic(TopicKind::WelcomeMessagesV1, b"installation"),
                ],
            })
        );
        assert_eq!(
            conn.next().await,
            Some(BidiEvent::CatchUpComplete { mutate_id: 11 })
        );
        assert_eq!(
            conn.next().await,
            Some(BidiEvent::GroupMessages(vec![group_msg(7, b"live")]))
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn inbound_error_closes_the_connection() {
        let (api, mut server) = mock_pair();
        let mut conn = BidiConnection::open(&api, Mutate::default()).await?;
        server.next_request().await;

        server
            .to_client
            .send(Err(ApiClientError::client(Boom)))
            .unwrap();
        assert_eq!(conn.next().await, None);
    }

    /// The whole point of the push-down: when the server closes the response
    /// side, the actor (which owns *both* halves) tears everything down — so a
    /// later `mutate`/`probe` reports `Closed` by ownership, never silently
    /// enqueueing into a dead stream. The transport's drainer keeps the wire
    /// channel itself open, so only the actor's exit can produce this.
    #[xmtp_common::test(unwrap_try = true)]
    async fn closing_inbound_tears_down_sends() {
        let (api, mut server) = mock_pair();
        let mut conn = BidiConnection::open(&api, Mutate::default()).await?;
        server.next_request().await; // initial mutate

        // Drop the server: its `to_client` closes (inbound ends → actor exits),
        // while the transport's outbound drainer survives and keeps the wire
        // channel open.
        drop(server);
        assert_eq!(conn.next().await, None, "actor observed end-of-stream");

        assert!(
            matches!(conn.mutate(Mutate::default()).await, Err(BidiError::Closed)),
            "mutate after teardown must report Closed, not silently enqueue"
        );
        assert!(
            matches!(conn.probe().await, Err(BidiError::Closed)),
            "probe after teardown must report Closed"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn concurrent_mutate_and_probe_both_reach_the_wire() {
        let (api, mut server) = mock_pair();
        let conn = BidiConnection::open(&api, Mutate::default()).await?;
        server.next_request().await; // initial mutate

        let m = Mutate {
            removes: vec![wire_topic(TopicKind::GroupMessagesV1, b"group")],
            ..Default::default()
        };

        // Fire a mutate and a probe concurrently; both must reach the wire (in
        // some order), and the probe resolves once its pong returns.
        let expected = m.clone();
        let server_side = async {
            let mut saw_mutate = false;
            let mut pong_nonce = None;
            for _ in 0..2 {
                match server.next_request().await {
                    subscribe_request::v1::Request::Mutate(sent) => {
                        assert_eq!(sent, expected);
                        saw_mutate = true;
                    }
                    subscribe_request::v1::Request::Ping(ping) => pong_nonce = Some(ping.nonce),
                    other => panic!("unexpected frame: {other:?}"),
                }
            }
            server.send(subscribe_response::v1::Response::Pong(Pong {
                nonce: pong_nonce.expect("probe must send a ping"),
            }));
            saw_mutate
        };

        let (mutate_res, probe_res, saw_mutate) =
            futures::join!(conn.mutate(m), conn.probe(), server_side);
        assert!(mutate_res.is_ok());
        assert!(matches!(probe_res, Ok(())));
        assert!(saw_mutate, "the mutate must reach the wire");
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn probe_within_times_out_when_no_pong() {
        let (api, mut server) = mock_pair();
        let conn = BidiConnection::open(&api, Mutate::default()).await?;
        server.next_request().await; // initial mutate

        // The server never answers the probe's ping. With a tight bound a caller
        // (e.g. a notification handler) gets a prompt `ProbeTimedOut` rather than
        // waiting out the keepalive-derived default.
        let result = conn.probe_within(Duration::from_millis(100)).await;
        assert!(matches!(result, Err(BidiError::ProbeTimedOut)));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn default_probe_timeout_tracks_server_keepalive() {
        let (api, mut server) = mock_pair();
        let mut conn = BidiConnection::open(&api, Mutate::default()).await?;
        server.next_request().await; // initial mutate

        // Before `Started`: the 30s fallback × N.
        assert_eq!(
            conn.default_probe_timeout(),
            Duration::from_millis(
                u64::from(DEFAULT_KEEPALIVE_MS) * u64::from(PROBE_TIMEOUT_MULTIPLIER)
            )
        );

        // After `Started` advertises a cadence, the default tracks it.
        server.send(started(5_000, vec![]));
        assert_eq!(
            conn.next().await,
            Some(BidiEvent::Started {
                keepalive_interval_ms: 5_000,
                capabilities: vec![],
            })
        );
        assert_eq!(
            conn.default_probe_timeout(),
            Duration::from_millis(5_000 * u64::from(PROBE_TIMEOUT_MULTIPLIER))
        );
    }

    /// A transport that delivers inbound but never drains the wire (it holds the
    /// outbound stream unpolled), so a test can wedge the request half.
    struct WedgedWireApi {
        inbound: Mutex<Option<mpsc::UnboundedReceiver<Result<SubscribeResponse, ApiClientError>>>>,
        held: Mutex<Option<BoxStream<'static, SubscribeRequest>>>,
    }

    #[xmtp_common::async_trait]
    impl XmtpMlsBidiStreams for WedgedWireApi {
        type SubscribeStream = BoxStream<'static, Result<SubscribeResponse, ApiClientError>>;
        type Error = ApiClientError;

        async fn subscribe_bidi(
            &self,
            requests: BoxStream<'static, SubscribeRequest>,
        ) -> Result<Self::SubscribeStream, Self::Error> {
            // Hold the outbound stream and never poll it: the wire fills and stays
            // full.
            *self.held.lock().unwrap() = Some(requests);
            let mut inbound = self
                .inbound
                .lock()
                .unwrap()
                .take()
                .expect("subscribe_bidi called twice");
            Ok(Box::pin(futures::stream::poll_fn(move |cx| {
                inbound.poll_recv(cx)
            })))
        }
    }

    /// With the old `send().await`-in-`select!` design, a full wire parked the
    /// actor and it stopped reading inbound — so an auto-pong (or any event)
    /// behind the wedge was lost. The queue + `reserve` branch must keep inbound
    /// flowing while the wire is backed up.
    #[xmtp_common::test(unwrap_try = true)]
    async fn busy_wire_does_not_stall_inbound() {
        let (to_client, inbound) = mpsc::unbounded_channel();
        let api = WedgedWireApi {
            inbound: Mutex::new(Some(inbound)),
            held: Mutex::new(None),
        };
        let mut conn = BidiConnection::open(&api, Mutate::default()).await?;

        // Saturate the wire: the transport never drains it, so the pre-seeded
        // initial Mutate plus a batch of mutates fill the wire and back up the
        // queue. None of these block the caller (command channel + queue have
        // room), and crucially none park the actor.
        for _ in 0..(WIRE_BUFFER * 2) {
            conn.mutate(Mutate::default()).await?;
        }

        // Despite the wedged wire, a server frame is still read and surfaced.
        to_client
            .send(Ok(SubscribeResponse {
                version: Some(subscribe_response::Version::V1(subscribe_response::V1 {
                    response: Some(started(7_000, vec![])),
                })),
            }))
            .unwrap();
        assert_eq!(
            conn.next().await,
            Some(BidiEvent::Started {
                keepalive_interval_ms: 7_000,
                capabilities: vec![],
            })
        );
    }

    /// A permanently wedged wire must not buffer forever: once the backlog blows
    /// past the cap the actor gives up and tears down, so the consumer re-opens
    /// rather than the queue growing without bound.
    #[xmtp_common::test(unwrap_try = true)]
    async fn gives_up_when_wire_wedged_past_backlog_cap() {
        let (to_client, inbound) = mpsc::unbounded_channel();
        let api = WedgedWireApi {
            inbound: Mutex::new(Some(inbound)),
            held: Mutex::new(None),
        };
        let mut conn = BidiConnection::open(&api, Mutate::default()).await?;

        // Flood server pings: the auto-pongs fill the never-draining wire, then
        // pile in the queue until it exceeds the cap and the actor gives up.
        for _ in 0..(MAX_PENDING_FRAMES + WIRE_BUFFER + 2) {
            to_client
                .send(Ok(SubscribeResponse {
                    version: Some(subscribe_response::Version::V1(subscribe_response::V1 {
                        response: Some(subscribe_response::v1::Response::Ping(Ping { nonce: 1 })),
                    })),
                }))
                .unwrap();
        }

        // The connection tears down instead of buffering without bound.
        assert_eq!(conn.next().await, None);
        assert!(matches!(
            conn.mutate(Mutate::default()).await,
            Err(BidiError::Closed)
        ));
    }

    /// A `Finish` queued behind a backlogged wire must still be processed — the
    /// commands branch is deliberately ungated so a wedged wire can't starve it
    /// (with a `pending`-room gate, the half-close would never reach the
    /// transport and `next()` would hang forever). Regression test for that gate:
    /// after `finish`, the flush budget expires, the un-flushable backlog is
    /// dropped, and the request half closes — so draining the held outbound
    /// stream yields exactly the frames the wire had already accepted, then ends.
    #[xmtp_common::test(unwrap_try = true)]
    async fn finish_is_processed_under_wire_backpressure() {
        let (_to_client, inbound) = mpsc::unbounded_channel();
        let api = WedgedWireApi {
            inbound: Mutex::new(Some(inbound)),
            held: Mutex::new(None),
        };
        let conn = BidiConnection::open(&api, Mutate::default()).await?;

        // Back the wire up well past its depth: the initial Mutate plus the
        // first WIRE_BUFFER - 1 mutates fill the wire; the rest park in the
        // actor's pending queue.
        for _ in 0..(WIRE_BUFFER * 2) {
            conn.mutate(Mutate::default()).await?;
        }

        // The half-close must be accepted AND processed despite the backlog.
        conn.finish().await?;

        // Give the drain's flush budget time to expire (the wire never drains,
        // so only the budget can end the flush), then take the held outbound
        // stream: it must yield the WIRE_BUFFER frames the wire had accepted and
        // then END — proof the actor dropped the request half rather than
        // parking forever with Finish stuck behind the backlog.
        tokio::time::sleep(Duration::from_secs(2)).await;
        let mut outbound = api.held.lock().unwrap().take().expect("wire was opened");
        let mut flushed = 0usize;
        while let Some(_frame) = outbound.next().await {
            flushed += 1;
        }
        assert_eq!(
            flushed, WIRE_BUFFER,
            "only the already-accepted wire frames flush; the backlog is dropped \
             and the request half closes"
        );
    }

    /// After `finish`, the request half is half-closed: `mutate`/`probe` must
    /// report `Closed` synchronously. Without the handle-side flag they would
    /// race the actor — `Command::Finish` is still in flight, the command receiver
    /// is briefly alive, and a FIFO-following `mutate` would be accepted into the
    /// buffer (returning `Ok`) only to be dropped unread when the actor drains.
    #[xmtp_common::test(unwrap_try = true)]
    async fn mutate_and_probe_report_closed_after_finish() {
        let (api, mut server) = mock_pair();
        let conn = BidiConnection::open(&api, Mutate::default()).await?;
        server.next_request().await; // initial mutate

        conn.finish().await?;

        assert!(
            matches!(conn.mutate(Mutate::default()).await, Err(BidiError::Closed)),
            "mutate after finish must report Closed, not land in the buffer"
        );
        assert!(
            matches!(conn.probe().await, Err(BidiError::Closed)),
            "probe after finish must report Closed"
        );
    }

    /// A `probe` in flight when `finish` half-closes must resolve to `Closed`, not
    /// hang: the actor drops the probe-ack senders before the (possibly long)
    /// inbound drain. Put the probe's ping on the wire, then half-close, and assert
    /// the probe resolves `Closed` rather than waiting out its timeout.
    #[xmtp_common::test(unwrap_try = true)]
    async fn finish_resolves_in_flight_probe_to_closed() {
        let (api, mut server) = mock_pair();
        let conn = BidiConnection::open(&api, Mutate::default()).await?;
        server.next_request().await; // initial mutate

        // Once the probe's ping reaches the actor (registered, awaiting a pong that
        // never comes), half-close. A generous bound keeps a regression — a probe
        // that hangs through the drain — from waiting out the keepalive default.
        let driver = async {
            let req = server.next_request().await;
            assert!(
                matches!(req, subscribe_request::v1::Request::Ping(_)),
                "probe must put a ping on the wire"
            );
            conn.finish().await
        };
        let (probe_res, finish_res) =
            futures::join!(conn.probe_within(Duration::from_secs(5)), driver);
        assert!(
            matches!(probe_res, Err(BidiError::Closed)),
            "an in-flight probe must resolve Closed when finish half-closes, got {probe_res:?}"
        );
        assert!(finish_res.is_ok());
    }
}
