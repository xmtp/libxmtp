//! Backend subscription frames for the connection actor and topic ledger.

use crate::queries::bidi::{BidiBinding, Connection, Event, Inbound};
use crate::queries::bidi_transport::TransportBinding;
use xmtp_proto::api_client::XmtpMlsBidiStreams;
use xmtp_proto::backend_v1::{
    self, Ping, Pong, ServerEnvelope, SubscribeRequest, SubscribeResponse, subscribe_request,
    subscribe_request::Update, subscribe_response,
};
use xmtp_proto::types::{Topic, TopicKind};

/// The backend uses one scalar cursor per topic.
pub struct BackendBinding;

pub type BidiConnection = Connection<BackendBinding>;
pub type BidiEvent = Event<ServerEnvelope, ServerEnvelope>;

fn request_frame(request: subscribe_request::Request) -> SubscribeRequest {
    SubscribeRequest {
        request: Some(request),
    }
}

fn topic_from_wire(topic: &backend_v1::Topic) -> Option<Topic> {
    // Topic::try_from checks length. Check the kind before calling Topic::kind.
    TopicKind::try_from(*topic.topic.first()?).ok()?;
    Topic::try_from(topic.topic.clone()).ok()
}

fn envelope_topic(envelope: &ServerEnvelope) -> Option<Topic> {
    topic_from_wire(envelope.meta.as_ref()?.topic.as_ref()?)
}

impl BidiBinding for BackendBinding {
    type Request = SubscribeRequest;
    type Response = SubscribeResponse;
    type Mutate = Update;
    type GroupMessage = ServerEnvelope;
    type WelcomeMessage = ServerEnvelope;

    fn mutate_frame(update: Update) -> SubscribeRequest {
        request_frame(subscribe_request::Request::Update(update))
    }

    fn ping_frame(nonce: u64) -> SubscribeRequest {
        request_frame(subscribe_request::Request::Ping(Ping { nonce }))
    }

    fn pong_frame(nonce: u64) -> SubscribeRequest {
        request_frame(subscribe_request::Request::Pong(Pong { nonce }))
    }

    fn handle(response: SubscribeResponse) -> Inbound<ServerEnvelope, ServerEnvelope> {
        use subscribe_response::Response;
        match response.response {
            Some(Response::Started(started)) => Inbound::Emit(Event::Started {
                keepalive_interval_ms: started.keepalive_interval_ms,
            }),
            Some(Response::Applied(applied)) => {
                let targets: Option<Vec<_>> = applied
                    .added_targets
                    .into_iter()
                    .map(|target| {
                        Some((
                            topic_from_wire(target.topic.as_ref()?)?,
                            target.through_sequence_id,
                        ))
                    })
                    .collect();
                match targets {
                    Some(targets) => Inbound::Emit(Event::Applied {
                        id: applied.id,
                        targets,
                    }),
                    None => Inbound::Invalid("Applied topic"),
                }
            }
            Some(Response::Messages(messages)) => {
                let mut group = Vec::new();
                let mut welcome = Vec::new();
                for envelope in messages.envelopes {
                    match envelope_topic(&envelope).map(|topic| topic.kind()) {
                        Some(TopicKind::GroupMessagesV1 | TopicKind::IdentityUpdatesV1) => {
                            group.push(envelope)
                        }
                        Some(TopicKind::WelcomeMessagesV1) => welcome.push(envelope),
                        _ => return Inbound::Invalid("envelope topic"),
                    }
                }
                Inbound::Messages { group, welcome }
            }
            Some(Response::Ping(ping)) => Inbound::Ping(ping.nonce),
            Some(Response::Pong(pong)) => Inbound::Pong(pong.nonce),
            None => Inbound::Invalid("response"),
        }
    }
}

impl TransportBinding for BackendBinding {
    type Cursor = u64;

    fn build_mutate(
        adds: impl IntoIterator<Item = (Topic, u64)>,
        removes: impl IntoIterator<Item = Topic>,
        id: u64,
    ) -> Update {
        Update {
            id,
            adds: adds
                .into_iter()
                .map(|(topic, sequence_id)| backend_v1::TopicQuery {
                    topic: Some(backend_v1::Topic {
                        topic: topic.to_bytes().into_vec(),
                    }),
                    cursor: Some(backend_v1::Cursor { sequence_id }),
                })
                .collect(),
            removes: removes
                .into_iter()
                .map(|topic| backend_v1::Topic {
                    topic: topic.to_bytes().into_vec(),
                })
                .collect(),
        }
    }

    fn group_topic(envelope: &ServerEnvelope) -> Option<Topic> {
        envelope_topic(envelope)
    }

    fn welcome_topic(envelope: &ServerEnvelope) -> Option<Topic> {
        envelope_topic(envelope)
    }

    fn group_cursor(envelope: &ServerEnvelope) -> Option<u64> {
        Some(envelope.meta.as_ref()?.cursor.as_ref()?.sequence_id)
    }

    fn welcome_cursor(envelope: &ServerEnvelope) -> Option<u64> {
        Self::group_cursor(envelope)
    }

    fn group_envelope(envelope: &ServerEnvelope) -> &ServerEnvelope {
        envelope
    }
    fn welcome_envelope(envelope: &ServerEnvelope) -> &ServerEnvelope {
        envelope
    }

    fn advance(position: &mut u64, delivered: u64) {
        *position = (*position).max(delivered);
    }

    fn covers(position: &u64, delivered: &u64) -> bool {
        delivered <= position
    }

    fn meet(a: u64, b: u64) -> u64 {
        a.min(b)
    }
}

impl BidiConnection {
    /// Open the backend subscription with the initial topic update.
    pub async fn open<A>(api: &A, initial: Update) -> Result<Self, A::Error>
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
        BidiError, COMMAND_BUFFER, DEFAULT_KEEPALIVE_MS, EVENT_BUFFER, MAX_PENDING_FRAMES,
        PROBE_TIMEOUT_MULTIPLIER, TryMutateError, WIRE_BUFFER,
    };
    use futures::StreamExt;
    use futures::stream::BoxStream;
    use std::sync::Mutex;
    use std::time::Duration;
    use tokio::sync::mpsc;
    use xmtp_proto::api::ApiClientError;
    use xmtp_proto::backend_v1::subscribe_request::Update as Mutate;
    use xmtp_proto::types::TopicKind;

    use crate::test::bidi::mock_pair;

    #[derive(Debug, thiserror::Error)]
    #[error("boom")]
    struct Boom;
    impl xmtp_common::RetryableError for Boom {
        fn is_retryable(&self) -> bool {
            false
        }
    }

    fn started(keepalive: u32) -> subscribe_response::Response {
        subscribe_response::Response::Started(subscribe_response::Started {
            keepalive_interval_ms: keepalive,
        })
    }

    fn applied(id: u64) -> subscribe_response::Response {
        subscribe_response::Response::Applied(subscribe_response::Applied {
            id,
            added_targets: vec![],
        })
    }

    fn wire_topic(kind: TopicKind, identifier: &[u8]) -> backend_v1::Topic {
        backend_v1::Topic {
            topic: kind.create(identifier).to_bytes().into_vec(),
        }
    }

    fn initial_mutate() -> Mutate {
        BackendBinding::build_mutate(
            [
                (TopicKind::GroupMessagesV1.create(b"group"), 5),
                (TopicKind::WelcomeMessagesV1.create(b"installation"), 0),
            ],
            [],
            11,
        )
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn open_sends_initial_mutate_and_emits_started() {
        let (api, mut server) = mock_pair();
        let mut conn = BidiConnection::open(&api, initial_mutate()).await?;

        let subscribe_request::Request::Update(sent) = server.next_request().await else {
            panic!("first frame must be the initial Mutate");
        };
        assert_eq!(sent, initial_mutate());

        server.send(started(30_000));
        assert_eq!(
            conn.next().await,
            Some(BidiEvent::Started {
                keepalive_interval_ms: 30_000,
            })
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn auto_pongs_server_ping_without_surfacing_it() {
        let (api, mut server) = mock_pair();
        let mut conn = BidiConnection::open(&api, Mutate::default()).await?;
        server.next_request().await; // initial mutate

        server.send(subscribe_response::Response::Ping(Ping { nonce: 42 }));
        let subscribe_request::Request::Pong(pong) = server.next_request().await else {
            panic!("server ping must be answered with a pong");
        };
        assert_eq!(pong.nonce, 42);

        // The ping/pong never reaches the consumer: the next event is the Started.
        server.send(started(15_000));
        assert_eq!(
            conn.next().await,
            Some(BidiEvent::Started {
                keepalive_interval_ms: 15_000,
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
            let subscribe_request::Request::Ping(ping) = server.next_request().await else {
                panic!("probe must send a Ping");
            };
            server.send(subscribe_response::Response::Pong(Pong {
                nonce: ping.nonce,
            }));
        };
        let (result, ()) = futures::join!(conn.probe(), server_side);
        assert!(matches!(result, Ok(())), "probe should resolve on its pong");

        // The correlating pong was consumed internally, never surfaced.
        server.send(started(10_000));
        assert_eq!(
            conn.next().await,
            Some(BidiEvent::Started {
                keepalive_interval_ms: 10_000,
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
        let subscribe_request::Request::Update(sent) = server.next_request().await else {
            panic!("mutate must reach the wire");
        };
        assert_eq!(sent, m);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn unknown_frames_close_with_a_protocol_error() {
        let (api, mut server) = mock_pair();
        let mut conn = BidiConnection::open(&api, Mutate::default()).await?;
        server.next_request().await; // initial mutate

        // Prost maps an unknown response kind to an absent oneof.
        server.send_raw(SubscribeResponse { response: None });
        assert!(conn.next().await.is_none());
        assert!(matches!(
            conn.failure().as_deref(),
            Some(crate::queries::bidi::ConnectionFailure::Protocol(
                "response"
            ))
        ));
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
                    subscribe_request::Request::Update(sent) => {
                        assert_eq!(sent, expected);
                        saw_mutate = true;
                    }
                    subscribe_request::Request::Ping(ping) => pong_nonce = Some(ping.nonce),
                    other => panic!("unexpected frame: {other:?}"),
                }
            }
            server.send(subscribe_response::Response::Pong(Pong {
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
        server.send(started(5_000));
        assert_eq!(
            conn.next().await,
            Some(BidiEvent::Started {
                keepalive_interval_ms: 5_000,
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

        fn host(&self) -> &str {
            "mock://bidi"
        }

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
                response: Some(started(7_000)),
            }))
            .unwrap();
        assert_eq!(
            conn.next().await,
            Some(BidiEvent::Started {
                keepalive_interval_ms: 7_000,
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
                    response: Some(subscribe_response::Response::Ping(Ping { nonce: 1 })),
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
        xmtp_common::time::sleep(Duration::from_secs(2)).await;
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
                matches!(req, subscribe_request::Request::Ping(_)),
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

    /// `try_mutate` hands the wave back with `Full` once the command buffer
    /// saturates behind an actor parked on a full event channel, and accepts
    /// again after the consumer drains. This is the primitive that lets a
    /// consumer that is also the sole event drainer stay non-blocking on the
    /// send side (the transport's deadlock discipline).
    #[xmtp_common::test(unwrap_try = true)]
    async fn try_mutate_reports_full_and_recovers_after_drain() {
        let (api, mut server) = mock_pair();
        let mut conn = BidiConnection::open(&api, initial_mutate()).await?;
        server.next_request().await; // initial mutate

        // Park the actor: overfill the event buffer so its emit blocks, which
        // stops command intake.
        for nonce in 0..(EVENT_BUFFER as u64 + 2) {
            server.send(applied(nonce));
        }
        // Saturate the command buffer. The actor drains a bounded handful of
        // these before it parks, so the loop terminates well under the bound.
        let mut accepted = 0usize;
        loop {
            match conn.try_mutate(initial_mutate()) {
                Ok(()) => {
                    accepted += 1;
                    assert!(
                        accepted <= EVENT_BUFFER + COMMAND_BUFFER + 8,
                        "actor never parked; try_mutate never reported Full"
                    );
                }
                Err(TryMutateError::Full(_)) => break,
                Err(TryMutateError::Closed(_)) => panic!("connection died under the flood"),
            }
            // Give the actor its chance to make progress between attempts.
            tokio::task::yield_now().await;
        }

        // Draining events unparks the actor, which resumes command intake.
        let mut recovered = false;
        for _ in 0..(EVENT_BUFFER + COMMAND_BUFFER) {
            assert!(
                conn.next().await.is_some(),
                "flooded events must all arrive"
            );
            if conn.try_mutate(initial_mutate()).is_ok() {
                recovered = true;
                break;
            }
        }
        assert!(recovered, "try_mutate must accept again after a drain");
    }

    /// After `finish`, `try_mutate` reports `Closed` and returns the wave.
    #[xmtp_common::test(unwrap_try = true)]
    async fn try_mutate_reports_closed_after_finish() {
        let (api, mut server) = mock_pair();
        let conn = BidiConnection::open(&api, initial_mutate()).await?;
        server.next_request().await; // initial mutate

        conn.finish().await?;
        assert!(matches!(
            conn.try_mutate(initial_mutate()),
            Err(TryMutateError::Closed(_))
        ));
    }
}
