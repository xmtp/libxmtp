//! XIP-83 bidirectional subscription *connection* (native-only).
//!
//! This sits one layer below the application: it owns a single bidi stream
//! end-to-end and is the **sole writer** of the request half. It auto-answers
//! server `Ping`s, correlates client liveness probes, and forwards only real
//! subscription events upward — keepalive never reaches the consumer.
//!
//! Owning *both* halves is the point. The actor holds the wire-outbound sender
//! and the inbound stream; when inbound dies it drops both, so the request half
//! tears down with it and any later `mutate`/`probe` fails with `Closed` by
//! channel ownership — never by silently enqueueing into a stream nothing reads.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use futures::StreamExt;
use tokio::sync::{mpsc, oneshot};
use xmtp_common::{AbortHandle, StreamHandle};
use xmtp_proto::api_client::XmtpMlsBidiStreams;
use xmtp_proto::mls_v1::subscribe_request::v1::Mutate;
use xmtp_proto::mls_v1::{
    GroupMessage, Ping, Pong, SubscribeRequest, SubscribeResponse, WelcomeMessage,
    subscribe_request, subscribe_response,
};
use xmtp_proto::types::Topic;

/// Wire-outbound depth. The actor is the sole writer; if the transport stalls,
/// `send().await` here backpressures the actor, which backpressures the command
/// queue — exactly the signal `probe` turns into a fast `Closed`.
const WIRE_BUFFER: usize = 64;
/// Caller→actor command depth.
const COMMAND_BUFFER: usize = 64;
/// Actor→caller event depth; large enough that a brief consumer stall doesn't
/// stall wire reads (and thus pong liveness).
const EVENT_BUFFER: usize = 1024;
/// XIP-83 client req 2 fallback keepalive, used until the server's `Started`
/// frame advertises its own cadence.
const DEFAULT_KEEPALIVE_MS: u32 = 30_000;
/// `N` from XIP-83 client req 2 (recommended 2–3): the *default* probe deadline
/// is this many keepalive intervals — generous enough not to false-positive a
/// slow-but-live link. Latency-sensitive callers (e.g. a notification handler)
/// pass a much smaller bound to [`BidiConnection::probe_within`].
const PROBE_TIMEOUT_MULTIPLIER: u32 = 3;
/// Hard cap on the outbound backlog. Past this, the wire has been wedged long
/// enough that buffering more is pointless — the transport isn't draining the
/// request half, so the link is effectively dead — and the actor gives up,
/// tearing down so the consumer re-opens from cursors on a fresh stream. Sits
/// well above the command-gated backlog (`WIRE_BUFFER`), so only a sustained
/// stall (pongs piling up on a half-open link) ever reaches it.
const MAX_PENDING_FRAMES: usize = WIRE_BUFFER * 2;

/// Events surfaced to the consumer, in wire order. `Ping`/`Pong` never appear —
/// liveness lives entirely inside the actor.
#[derive(Debug, Clone, PartialEq)]
pub enum BidiEvent {
    /// First frame on every stream; carries the server's keepalive cadence.
    Started {
        keepalive_interval_ms: u32,
        capabilities: Vec<i32>,
    },
    /// A `Mutate`'s adds are fully caught up; echoes the Mutate's `mutate_id`.
    CatchUpComplete {
        mutate_id: u64,
    },
    /// These topics just crossed from catch-up to live.
    TopicsLive {
        topics: Vec<Topic>,
    },
    GroupMessages(Vec<GroupMessage>),
    WelcomeMessages(Vec<WelcomeMessage>),
}

#[derive(Debug, thiserror::Error)]
pub enum BidiError {
    #[error("the bidi connection is closed; re-open and resume from durable cursors")]
    Closed,
    #[error("liveness probe timed out; treat the link as dead, drop it, and re-open")]
    ProbeTimedOut,
}

/// Submitted by the handle, performed by the actor (the sole wire writer).
enum Command {
    Mutate(Mutate),
    /// A client liveness probe. The actor sends the `Ping` and fires `ack` when
    /// the matching `Pong` returns; if the actor exits first, `ack` is dropped
    /// and the waiting [`BidiConnection::probe`] resolves to `Closed`.
    Probe {
        nonce: u64,
        ack: oneshot::Sender<()>,
    },
}

/// A handle to one open bidirectional subscription. Writing to the wire is the
/// actor's job; this only submits commands and reads events.
pub struct BidiConnection {
    commands: mpsc::Sender<Command>,
    events: mpsc::Receiver<BidiEvent>,
    probe_nonce: AtomicU64,
    /// The server's advertised keepalive cadence (ms), learned from `Started`;
    /// `0` until then. Shared with the actor, which sets it. Drives the default
    /// probe deadline so `probe` can self-bound without the caller re-deriving it.
    keepalive_ms: Arc<AtomicU32>,
    actor: Box<dyn AbortHandle>,
}

impl BidiConnection {
    /// Open the stream and send `initial` as the first Mutate (it names the
    /// initial topic set with per-topic resume cursors; XIP-83 client req 3).
    pub async fn open<A>(api: &A, initial: Mutate) -> Result<Self, A::Error>
    where
        A: XmtpMlsBidiStreams,
        A::SubscribeStream: 'static,
    {
        let (wire_out, mut wire_out_rx) = mpsc::channel(WIRE_BUFFER);
        let (commands_tx, commands_rx) = mpsc::channel(COMMAND_BUFFER);
        let (event_tx, events) = mpsc::channel(EVENT_BUFFER);

        // The first wire frame MUST be this Mutate (XIP-83 req 3). Seed it into
        // the fresh, empty wire channel before the transport or the actor can
        // write anything else; the receiver is still held here, so a fresh,
        // empty, bounded channel can neither be full nor closed — `try_send`
        // makes that "cannot block" invariant structural.
        wire_out
            .try_send(request_frame(subscribe_request::v1::Request::Mutate(
                initial,
            )))
            .expect("send into a fresh, empty channel whose receiver we still hold cannot fail");

        // Wrap the receiver as a `Stream` without pulling in `tokio-stream`:
        // `poll_recv` is exactly the poll fn a `Stream` needs.
        let outbound = futures::stream::poll_fn(move |cx| wire_out_rx.poll_recv(cx));
        let inbound = api.subscribe_bidi(Box::pin(outbound)).await?;

        let keepalive_ms = Arc::new(AtomicU32::new(0));
        let actor = xmtp_common::spawn(
            None,
            run_actor(
                Box::pin(inbound),
                wire_out,
                commands_rx,
                event_tx,
                keepalive_ms.clone(),
            ),
        );

        Ok(Self {
            commands: commands_tx,
            events,
            probe_nonce: AtomicU64::new(0),
            keepalive_ms,
            actor: actor.abort_handle(),
        })
    }

    /// Add/remove subscriptions in place. Awaits a free command slot
    /// (backpressure is right for a state change); returns `Closed` once the
    /// actor has stopped — the command receiver dies with it.
    pub async fn mutate(&self, mutate: Mutate) -> Result<(), BidiError> {
        self.commands
            .send(Command::Mutate(mutate))
            .await
            .map_err(|_| BidiError::Closed)
    }

    /// Probe the link (e.g. right after the process resumes) with the default
    /// deadline: `N ×` the server's advertised keepalive interval (a 30s-derived
    /// fallback until `Started` arrives). Resolves `Ok` on the matching `Pong`,
    /// `Closed` if the link is already torn down, or `ProbeTimedOut` if no pong
    /// arrives in time — the half-open case this whole mechanism exists to catch.
    pub async fn probe(&self) -> Result<(), BidiError> {
        self.probe_within(self.default_probe_timeout()).await
    }

    /// Probe with an explicit deadline. A latency-sensitive caller — say a push
    /// notification handler that must decide in a couple of seconds whether to
    /// reuse the connection or re-open — passes a bound far below the default.
    /// A tight bound trades occasional false `ProbeTimedOut`s for speed, which is
    /// safe: re-opening replays from durable cursors and discards duplicates.
    ///
    /// The timeout covers the whole probe — submitting the ping (so a stalled
    /// wire backpressuring the command queue can't park us) and awaiting the
    /// pong. We deliberately don't fail-fast on a full command queue: a transient
    /// burst of `mutate`s is "busy," not "dead."
    pub async fn probe_within(&self, timeout: Duration) -> Result<(), BidiError> {
        match tokio::time::timeout(timeout, self.probe_inner()).await {
            Ok(result) => result,
            Err(_elapsed) => Err(BidiError::ProbeTimedOut),
        }
    }

    async fn probe_inner(&self) -> Result<(), BidiError> {
        let nonce = self
            .probe_nonce
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        let (ack, ack_rx) = oneshot::channel();
        self.commands
            .send(Command::Probe { nonce, ack })
            .await
            .map_err(|_| BidiError::Closed)?;
        ack_rx.await.map_err(|_| BidiError::Closed)
    }

    fn default_probe_timeout(&self) -> Duration {
        let keepalive = match self.keepalive_ms.load(Ordering::Relaxed) {
            0 => DEFAULT_KEEPALIVE_MS,
            ms => ms,
        };
        Duration::from_millis(u64::from(keepalive) * u64::from(PROBE_TIMEOUT_MULTIPLIER))
    }

    /// Next event, in wire order. `None` means the connection ended (server
    /// close, network death, or reap) — resume from durable cursors on a fresh
    /// connection.
    ///
    /// Only resolves on an event or end-of-stream: a server that goes quiet
    /// without closing (a half-open link) leaves this pending. Detect that gap
    /// out of band with [`Self::probe`] plus a timeout, never by waiting here.
    pub async fn next(&mut self) -> Option<BidiEvent> {
        self.events.recv().await
    }
}

impl Drop for BidiConnection {
    fn drop(&mut self) {
        // Abort the actor so it cannot keep auto-ponging — a zombie keepalive
        // would hold the server-side subscription open forever. The abort drops
        // the actor's wire-outbound and inbound, cancelling the underlying
        // request and tearing the stream down in both directions.
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

/// Hand `frame` to the wire if it has room right now, else queue it for the
/// reserve branch to flush. Returns `false` when the backlog has grown past
/// [`MAX_PENDING_FRAMES`] — the wire is wedged and the actor should give up
/// rather than buffer forever.
///
/// Reaching the queue means the transport isn't draining the request half —
/// which on a healthy link should essentially never happen — so we warn the
/// first time we fall back to it (not on every frame: an existing backlog skips
/// straight to the queue without re-probing the wire).
#[must_use]
fn enqueue(
    wire_out: &mpsc::Sender<SubscribeRequest>,
    pending: &mut VecDeque<SubscribeRequest>,
    frame: SubscribeRequest,
) -> bool {
    if pending.is_empty() {
        match wire_out.try_send(frame) {
            Ok(()) => return true,
            Err(mpsc::error::TrySendError::Full(frame)) => {
                tracing::warn!(
                    "bidi wire is backpressured (transport not draining the request half); \
                     queuing outbound frames"
                );
                pending.push_back(frame);
            }
            // Transport gone: keep the frame queued so the reserve branch's `Err`
            // arm tears the actor down on the next iteration.
            Err(mpsc::error::TrySendError::Closed(frame)) => pending.push_back(frame),
        }
    } else {
        pending.push_back(frame);
    }
    if pending.len() > MAX_PENDING_FRAMES {
        tracing::warn!(
            backlog = pending.len(),
            "bidi wire wedged past the outbound backlog cap; giving up so the consumer re-opens"
        );
        return false;
    }
    true
}

/// The single owner of the wire: sole reader of inbound, sole writer of
/// outbound, sole producer of events. Caller commands and server frames are
/// multiplexed onto one outbound FIFO via an internal `pending` queue, drained
/// to the wire by a `reserve()` branch — so no send ever blocks the loop and a
/// busy wire can't delay an auto-pong. (Event sends to the *consumer* do still
/// await: a hopelessly slow consumer backpressures here, the intended path to
/// reap-then-resume — distinct from outbound/wire pressure, which is what the
/// queue handles.) It ends when the wire ends/errors or the handle goes away;
/// ending drops `wire_out` (the request half closes, tearing the stream down),
/// the `commands` receiver (so `mutate`/`probe` see `Closed`), the `events`
/// sender (so `next` ends), and any outstanding probe acks (so pending `probe`s
/// see `Closed`). One place, every teardown. It also gives up the same way if
/// the outbound backlog blows past [`MAX_PENDING_FRAMES`] — a wedged wire that
/// will never drain, so we tear down and let the consumer re-open.
async fn run_actor<S, E>(
    mut inbound: S,
    wire_out: mpsc::Sender<SubscribeRequest>,
    mut commands: mpsc::Receiver<Command>,
    events: mpsc::Sender<BidiEvent>,
    keepalive_ms: Arc<AtomicU32>,
) where
    S: futures::Stream<Item = Result<SubscribeResponse, E>> + Unpin,
    E: std::fmt::Display,
{
    // Outstanding client probes awaiting their `Pong`, keyed by nonce.
    let mut probes: HashMap<u64, oneshot::Sender<()>> = HashMap::new();
    // Outbound frames awaiting wire capacity. We drain these through a `reserve()`
    // branch below rather than `send().await` in a branch body — an `.await` in a
    // `select!` arm blocks the whole loop, so a backed-up wire would stall inbound
    // reads and delay auto-pongs (getting us reaped on an otherwise healthy link).
    let mut pending: VecDeque<SubscribeRequest> = VecDeque::new();

    loop {
        tokio::select! {
            // Hand one queued frame to the wire the moment it has capacity,
            // concurrently with everything else. This is what keeps a busy wire
            // from blocking the loop. Gated on a non-empty queue so we only
            // contend for a permit when there's something to flush.
            permit = wire_out.reserve(), if !pending.is_empty() => {
                match permit {
                    Ok(permit) => {
                        permit.send(pending.pop_front().expect("queue is non-empty"));
                        // Drain as much of the backlog as the wire will take right
                        // now, so a cleared stall flushes promptly.
                        while !pending.is_empty() {
                            match wire_out.try_reserve() {
                                Ok(permit) => {
                                    permit.send(pending.pop_front().expect("queue is non-empty"));
                                }
                                Err(_) => break, // wire full again, or closed
                            }
                        }
                    }
                    Err(_) => break, // transport gone
                }
            }
            // A command from the handle: mutate, or a liveness-probe ping. Accept
            // new commands only while the queue has room — that is the backpressure
            // to `mutate`, mirroring the old bounded wire. Inbound is never gated
            // this way, so auto-pong stays responsive under outbound pressure.
            cmd = commands.recv(), if pending.len() < WIRE_BUFFER => {
                let Some(cmd) = cmd else {
                    // Every handle dropped — nothing more will be sent.
                    break;
                };
                let queued = match cmd {
                    Command::Mutate(mutate) => enqueue(
                        &wire_out,
                        &mut pending,
                        request_frame(subscribe_request::v1::Request::Mutate(mutate)),
                    ),
                    Command::Probe { nonce, ack } => {
                        probes.insert(nonce, ack);
                        enqueue(
                            &wire_out,
                            &mut pending,
                            request_frame(subscribe_request::v1::Request::Ping(Ping { nonce })),
                        )
                    }
                };
                if !queued {
                    break; // wire wedged past the backlog cap — give up
                }
            }
            // A frame from the server. Always read, so liveness never stalls behind
            // outbound pressure.
            frame = inbound.next() => {
                let Some(frame) = frame else {
                    break; // inbound ended — the stream is closed
                };
                let response = match frame {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!("bidi subscription stream errored: {e}");
                        break;
                    }
                };
                let Some(subscribe_response::Version::V1(v1)) = response.version else {
                    // A version we did not speak; XIP-83 pins responses to the
                    // request version, so this is a server bug — skip, don't die.
                    tracing::warn!("bidi subscription received unknown response version");
                    continue;
                };
                use subscribe_response::v1::Response;
                match v1.response {
                    Some(Response::Ping(ping)) => {
                        // Auto-pong: hand it to the wire (or queue it FIFO behind a
                        // backlog). A busy wire delays but never drops the pong, and
                        // never blocks us from reading the next frame — but if the
                        // backlog has blown past the cap, the wire is wedged and we
                        // give up rather than keep piling pongs into a dead stream.
                        if !enqueue(
                            &wire_out,
                            &mut pending,
                            request_frame(subscribe_request::v1::Request::Pong(Pong {
                                nonce: ping.nonce,
                            })),
                        ) {
                            break;
                        }
                    }
                    Some(Response::Pong(pong)) => {
                        // Resolve the matching client probe. Unmatched pongs are
                        // ignored — never surfaced to the consumer.
                        if let Some(ack) = probes.remove(&pong.nonce) {
                            let _ = ack.send(());
                        }
                    }
                    Some(Response::Started(started)) => {
                        // Record the server's cadence before surfacing `Started`,
                        // so a consumer that probes in reaction to it already gets
                        // the keepalive-derived default deadline.
                        keepalive_ms.store(started.keepalive_interval_ms, Ordering::Relaxed);
                        if emit(&events, BidiEvent::Started {
                            keepalive_interval_ms: started.keepalive_interval_ms,
                            capabilities: started.capabilities,
                        }).await {
                            break;
                        }
                    }
                    Some(Response::CatchupComplete(complete)) => {
                        if emit(&events, BidiEvent::CatchUpComplete {
                            mutate_id: complete.mutate_id,
                        }).await {
                            break;
                        }
                    }
                    Some(Response::TopicsLive(live)) => {
                        // Parse kind-prefixed wire bytes into typed `Topic`s,
                        // validating each kind byte. A malformed topic is a
                        // server bug on an informational frame, so skip it (with
                        // a warn) rather than kill the connection.
                        let topics = live
                            .topics
                            .into_iter()
                            .filter_map(|bytes| match Topic::try_from(bytes) {
                                Ok(topic) => Some(topic),
                                Err(e) => {
                                    tracing::warn!("skipping malformed TopicsLive topic: {e}");
                                    None
                                }
                            })
                            .collect();
                        if emit(&events, BidiEvent::TopicsLive { topics }).await {
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
                    // An arm a future server revision added: informational frames
                    // are safe to skip (delivery correctness never depends on them).
                    None => continue,
                }
            }
        }
    }
    // One exit log for every reason (wire ended/errored, or handle gone). The
    // error a caller sees is just `Closed`; the "why" lives here in the logs.
    tracing::debug!("bidi subscription actor stopped");
    // `wire_out`, `commands`, `events`, `probes`, and the `pending` queue drop
    // here — that *is* the teardown in both directions.
}

/// Send an event to the consumer; awaits a free slot (the backpressure that
/// stalls wire reads once [`EVENT_BUFFER`] fills) and returns true when the
/// consumer is gone and the actor should shut down.
async fn emit(events: &mpsc::Sender<BidiEvent>, event: BidiEvent) -> bool {
    events.send(event).await.is_err()
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::BoxStream;
    use std::sync::Mutex;
    use xmtp_proto::api::ApiClientError;
    use xmtp_proto::mls_v1::subscribe_request::v1::mutate::Subscription;
    use xmtp_proto::mls_v1::{group_message, welcome_message};
    use xmtp_proto::types::TopicKind;

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
        let mut topic = vec![kind as u8];
        topic.extend_from_slice(identifier);
        topic
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
}
