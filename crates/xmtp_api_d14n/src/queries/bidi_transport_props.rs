//! Property-based model test of the bidi transport ledger.
//!
//! The scripted tests in `bidi_transport.rs` pin named scenarios; the live
//! fuzz in `xmtp_mls` proves the stack against the real node but cannot
//! shrink a failure. This sits between them: proptest generates random op
//! sequences (publish / lease / drop / serve / partial-serve / wire-kill),
//! a deterministic in-memory server model plays the XIP-83 contract back
//! over the same mock wire the scripted tests use, and a reference model
//! asserts the ledger's delivery contract per lease:
//!
//! - strictly increasing cursors per topic, nothing at-or-below the floor
//!   (order + exactly-once in one sweep),
//! - a lease alive at the end holds *exactly* the log suffix above its
//!   floor and exactly one `CatchUpComplete`,
//! - frames only for topics the lease asked for.
//!
//! Because the server is a model, completeness needs no sentinel bounding —
//! the log IS ground truth — and because everything is in-memory, proptest
//! gets what it exists for: thousands of schedules and real shrinking to a
//! minimal counterexample. Server-side, the model also asserts the client's
//! wire behavior (unique wave ids per connection, no pings at a disabled
//! keepalive).
//!
//! Model fidelity notes, matching behavior established against node-go
//! (validated at ≥ `6e0feb5f`, the tag-serving line — re-check these if
//! the server's wave semantics change) by the live fuzz: every Mutate is
//! acked (removes-only immediately, adds via
//! wave serve); a wave owns a topic only when its floor is below the
//! topic's position (equal-or-higher re-adds are waveless no-ops); owned
//! topics get no live frames until every owning wave completes (yank
//! overlap re-serves ranges — the ledger must dedup per lease); waves die
//! with their connection, and a reconnect's initial Mutate re-registers
//! everything at the transport's own floors.

#![allow(clippy::unwrap_used)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::FutureExt;
use futures::stream::BoxStream;
use proptest::prelude::*;

use super::{
    BidiConnection, BidiTransport, DEFAULT_LEASE_DEPTH, LeaseEvent, OpenError, TopicLease,
    V3Binding,
};
use xmtp_proto::api::ApiClientError;
use xmtp_proto::api_client::XmtpMlsBidiStreams;
use xmtp_proto::mls_v1::subscribe_request::v1::Mutate;
use xmtp_proto::mls_v1::{
    GroupMessage, SubscribeRequest, SubscribeResponse, group_message, subscribe_request,
    subscribe_response,
};
use xmtp_proto::types::Topic;

const N_TOPICS: usize = 3;
/// Keepalive high enough that the client never pings within a test case —
/// a Ping reaching the model is an assertion failure, not a protocol turn.
const NO_KEEPALIVE: u32 = 3_600_000;
/// Wall-clock bound on any settle/wait loop; hitting it IS the failure.
const STALL: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// Mock wire (same shape as the scripted suite's, self-contained here so this
// file never conflicts with edits to `bidi_transport.rs`).
// ---------------------------------------------------------------------------

struct MockApi {
    inbound: Mutex<
        Option<tokio::sync::mpsc::UnboundedReceiver<Result<SubscribeResponse, ApiClientError>>>,
    >,
    captured: tokio::sync::mpsc::UnboundedSender<SubscribeRequest>,
}

struct MockServer {
    to_client: tokio::sync::mpsc::UnboundedSender<Result<SubscribeResponse, ApiClientError>>,
    from_client: tokio::sync::mpsc::UnboundedReceiver<SubscribeRequest>,
}

#[xmtp_common::async_trait]
impl XmtpMlsBidiStreams for MockApi {
    type SubscribeStream = BoxStream<'static, Result<SubscribeResponse, ApiClientError>>;
    type Error = ApiClientError;

    fn host(&self) -> &str {
        "mock://bidi"
    }

    async fn subscribe_bidi(
        &self,
        requests: BoxStream<'static, SubscribeRequest>,
    ) -> Result<Self::SubscribeStream, Self::Error> {
        let captured = self.captured.clone();
        xmtp_common::spawn(None, async move {
            let mut requests = requests;
            while let Some(frame) = futures::StreamExt::next(&mut requests).await {
                let _ = captured.send(frame);
            }
        });
        let mut inbound = self
            .inbound
            .lock()
            .unwrap()
            .take()
            .expect("subscribe_bidi called twice on one mock session");
        Ok(Box::pin(futures::stream::poll_fn(move |cx| {
            inbound.poll_recv(cx)
        })))
    }
}

impl MockServer {
    fn send(&self, response: subscribe_response::v1::Response) {
        // The session may die under the client mid-send (wire kill racing a
        // reconnect); losing frames there is exactly a severed TCP stream.
        let _ = self.to_client.send(Ok(SubscribeResponse {
            version: Some(subscribe_response::Version::V1(subscribe_response::V1 {
                response: Some(response),
            })),
        }));
    }
}

type Servers = Arc<Mutex<Vec<MockServer>>>;

/// A transport whose opener mints a scripted session per open, greets it
/// with `Started` immediately (so `open`/`lease` never deadlock on the
/// driver), and parks the server end for the driver to adopt.
fn model_transport() -> (BidiTransport<V3Binding>, Servers) {
    let servers: Servers = Arc::default();
    let sink = servers.clone();
    let transport = BidiTransport::new(
        move |initial| {
            let (to_client, inbound) = tokio::sync::mpsc::unbounded_channel();
            let (captured, from_client) = tokio::sync::mpsc::unbounded_channel();
            let api = MockApi {
                inbound: Mutex::new(Some(inbound)),
                captured,
            };
            let server = MockServer {
                to_client,
                from_client,
            };
            server.send(subscribe_response::v1::Response::Started(
                subscribe_response::v1::Started {
                    keepalive_interval_ms: NO_KEEPALIVE,
                    capabilities: vec![],
                },
            ));
            sink.lock().unwrap().push(server);
            async move {
                BidiConnection::open(&api, initial)
                    .await
                    .map_err(OpenError::new)
            }
        },
        false,
    );
    (transport, servers)
}

fn gid(topic: usize) -> [u8; 16] {
    [topic as u8 + 1; 16]
}

fn group_msg(id: u64, topic: usize) -> GroupMessage {
    GroupMessage {
        version: Some(group_message::Version::V1(group_message::V1 {
            id,
            group_id: gid(topic).to_vec(),
            data: vec![0xda; 4],
            ..Default::default()
        })),
    }
}

fn replay(mutate_id: u64, group: Vec<GroupMessage>) -> subscribe_response::v1::Response {
    subscribe_response::v1::Response::Messages(subscribe_response::v1::Messages {
        group_messages: group,
        welcome_messages: vec![],
        mutate_id,
    })
}

fn catchup_complete(mutate_id: u64) -> subscribe_response::v1::Response {
    subscribe_response::v1::Response::CatchupComplete(subscribe_response::v1::CatchupComplete {
        mutate_id,
    })
}

// ---------------------------------------------------------------------------
// Generated schedule
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum FloorClass {
    Zero,
    Mid,
    AtEdge,
    Ahead,
}

#[derive(Debug, Clone)]
enum Op {
    /// Append `n` messages to `topic`'s log; live-deliver if unheld.
    Publish { topic: usize, n: u8 },
    /// A new lease over the given topics (deduped, floors resolved against
    /// the log at execution time).
    Lease { asks: Vec<(usize, FloorClass)> },
    /// Deliberately drop an alive lease (picked by index modulo).
    DropLease { pick: u8 },
    /// Serve the oldest pending wave to completion and ack it.
    ServeWave,
    /// Serve roughly half of the oldest pending wave, no ack — a later
    /// `ServeWave` finishes it; a `KillWire` orphans it mid-replay.
    ServePartial,
    /// Sever the wire; the transport must reconnect and re-establish.
    KillWire,
}

fn floor_class() -> impl Strategy<Value = FloorClass> {
    prop_oneof![
        2 => Just(FloorClass::Zero),
        2 => Just(FloorClass::Mid),
        1 => Just(FloorClass::AtEdge),
        1 => Just(FloorClass::Ahead),
    ]
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (0..N_TOPICS, 1..4u8).prop_map(|(topic, n)| Op::Publish { topic, n }),
        3 => proptest::collection::vec((0..N_TOPICS, floor_class()), 1..3)
            .prop_map(|asks| Op::Lease { asks }),
        1 => any::<u8>().prop_map(|pick| Op::DropLease { pick }),
        3 => Just(Op::ServeWave),
        1 => Just(Op::ServePartial),
        1 => Just(Op::KillWire),
    ]
}

// ---------------------------------------------------------------------------
// Server model + reference expectations
// ---------------------------------------------------------------------------

struct WaveAdd {
    topic: usize,
    /// Highest cursor already covered for this add — floor at registration,
    /// advanced by partial serves.
    served_upto: u64,
    /// Floor was provably below the topic's position: the add moved the
    /// topic into the wave and holds its live lane until the ack.
    owned: bool,
}

struct PendingWave {
    id: u64,
    adds: Vec<WaveAdd>,
}

/// One lease under test: the handle plus what the reference model expects
/// of it.
struct LeaseRef {
    lease: Option<TopicLease<V3Binding>>,
    floors: HashMap<usize, u64>,
    got: HashMap<usize, Vec<u64>>,
    catch_ups: usize,
    dropped: bool,
}

struct Driver {
    transport: BidiTransport<V3Binding>,
    servers: Servers,
    session: Option<MockServer>,
    /// Per-topic message log — the server's persistent ground truth.
    log: Vec<Vec<u64>>,
    next_cursor: u64,
    /// Per-connection state: currently subscribed topics and unserved waves.
    subs: HashSet<usize>,
    pending: VecDeque<PendingWave>,
    wave_ids_seen: HashSet<u64>,
    leases: Vec<LeaseRef>,
    topic_index: HashMap<Vec<u8>, usize>,
    kills: usize,
}

impl Driver {
    fn new() -> Self {
        let (transport, servers) = model_transport();
        let topic_index = (0..N_TOPICS)
            .map(|t| {
                (
                    Topic::new_group_message(&gid(t)[..]).to_bytes().into_vec(),
                    t,
                )
            })
            .collect();
        Self {
            transport,
            servers,
            session: None,
            log: vec![Vec::new(); N_TOPICS],
            next_cursor: 0,
            subs: HashSet::new(),
            pending: VecDeque::new(),
            wave_ids_seen: HashSet::new(),
            leases: Vec::new(),
            topic_index,
            kills: 0,
        }
    }

    fn position(&self, topic: usize) -> u64 {
        self.log[topic].last().copied().unwrap_or(0)
    }

    fn held(&self, topic: usize) -> bool {
        self.pending
            .iter()
            .any(|w| w.adds.iter().any(|a| a.topic == topic && a.owned))
    }

    fn send(&self, response: subscribe_response::v1::Response) {
        if let Some(session) = &self.session {
            session.send(response);
        }
    }

    fn on_client_frame(&mut self, frame: SubscribeRequest) {
        let Some(subscribe_request::Version::V1(v1)) = frame.version else {
            panic!("client sent unknown request version");
        };
        match v1.request.expect("client sent empty request") {
            subscribe_request::v1::Request::Mutate(mutate) => self.on_mutate(mutate),
            other => panic!("unexpected client frame: {other:?}"),
        }
    }

    fn on_mutate(&mut self, mutate: Mutate) {
        assert_ne!(mutate.mutate_id, 0, "client sent an untagged Mutate");
        assert!(
            self.wave_ids_seen.insert(mutate.mutate_id),
            "client reused wave id {} on one connection",
            mutate.mutate_id
        );
        for removed in &mutate.removes {
            let topic = *self
                .topic_index
                .get(removed)
                .expect("client removed an unknown topic");
            self.subs.remove(&topic);
        }
        let adds: Vec<WaveAdd> = mutate
            .adds
            .iter()
            .map(|sub| {
                let topic = *self
                    .topic_index
                    .get(&sub.topic)
                    .expect("client added an unknown topic");
                self.subs.insert(topic);
                WaveAdd {
                    topic,
                    served_upto: sub.id_cursor,
                    owned: sub.id_cursor < self.position(topic),
                }
            })
            .collect();
        if adds.is_empty() {
            // Removes-only: always-ack, echoing the minted id immediately.
            self.send(catchup_complete(mutate.mutate_id));
        } else {
            self.pending.push_back(PendingWave {
                id: mutate.mutate_id,
                adds,
            });
        }
    }

    fn publish(&mut self, topic: usize, n: u8) {
        for _ in 0..n {
            self.next_cursor += 1;
            let id = self.next_cursor;
            self.log[topic].push(id);
            if self.session.is_some() && self.subs.contains(&topic) && !self.held(topic) {
                self.send(replay(0, vec![group_msg(id, topic)]));
            }
        }
    }

    /// Serve the front wave's remaining replay; ack and pop unless partial.
    ///
    /// Per-topic cursor order is the contract; CROSS-topic order within a
    /// wave is not (the live fuzz confirmed the node interleaves freely).
    /// Even waves replay in global cursor order, odd waves grouped per
    /// topic — so the ledger's tolerance for both shapes stays exercised.
    fn serve_front(&mut self, partial: bool) {
        let Some(wave) = self.pending.front_mut() else {
            return;
        };
        let wave_id = wave.id;
        let mut owed: Vec<(u64, usize)> = Vec::new();
        for (slot, add) in wave.adds.iter().enumerate() {
            // An owning add converges to the live edge at serve time (its
            // held publishes ride the replay). A non-owning add is a
            // waveless no-op: it replays NOTHING — its topic's publishes
            // flowed live from registration — and only shares the ack.
            // Serving it a suffix would re-serve live-delivered ids from a
            // wave that never owned them, which no real server does.
            if !add.owned {
                continue;
            }
            owed.extend(
                self.log[add.topic]
                    .iter()
                    .copied()
                    .filter(|id| *id > add.served_upto)
                    .map(|id| (id, slot)),
            );
        }
        if wave_id % 2 == 0 {
            owed.sort_unstable();
        }
        let take = if partial { owed.len() / 2 } else { owed.len() };
        let mut frames = Vec::new();
        for (id, slot) in &owed[..take] {
            let add = &mut wave.adds[*slot];
            add.served_upto = (*id).max(add.served_upto);
            frames.push(group_msg(*id, add.topic));
        }
        for frame in frames {
            self.send(replay(wave_id, vec![frame]));
        }
        if !partial {
            self.pending.pop_front();
            self.send(catchup_complete(wave_id));
        }
    }

    /// Adopt a freshly opened session: per-connection server state resets;
    /// the log survives (it is storage, not connection state).
    fn adopt_sessions(&mut self) -> bool {
        let mut adopted = false;
        loop {
            let next = {
                let mut servers = self.servers.lock().unwrap();
                if servers.is_empty() {
                    None
                } else {
                    Some(servers.remove(0))
                }
            };
            let Some(server) = next else { break };
            self.session = Some(server);
            self.subs.clear();
            self.pending.clear();
            self.wave_ids_seen.clear();
            adopted = true;
        }
        adopted
    }

    /// Pump everything until nothing moves for a few sweeps: adopt
    /// reconnects, fold client frames into the model, drain every lease.
    async fn settle(&mut self) {
        let deadline = tokio::time::Instant::now() + STALL;
        let mut stable = 0;
        while stable < 3 {
            assert!(
                tokio::time::Instant::now() < deadline,
                "settle never stabilized"
            );
            let mut progressed = self.adopt_sessions();
            let mut inbound = Vec::new();
            if let Some(session) = &mut self.session {
                loop {
                    match session.from_client.try_recv() {
                        Ok(frame) => inbound.push(frame),
                        Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                        Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                            // The transport closed the wire (e.g. last topic
                            // retired). A later lease re-opens it.
                            self.session = None;
                            break;
                        }
                    }
                }
            }
            for frame in inbound {
                progressed = true;
                self.on_client_frame(frame);
            }
            for slot in &mut self.leases {
                let Some(lease) = &mut slot.lease else {
                    continue;
                };
                while let Some(event) = lease.next().now_or_never() {
                    progressed = true;
                    match event {
                        None => {
                            // The transport ended this lease. Eager polling
                            // rules out backpressure, so only a deliberate
                            // drop path may do this — treat as ended.
                            slot.lease = None;
                            break;
                        }
                        Some(LeaseEvent::GroupMessages(batch)) => {
                            for message in &batch {
                                let Some(group_message::Version::V1(v1)) = &message.version else {
                                    panic!("undecodable frame reached a lease");
                                };
                                let topic = *self
                                    .topic_index
                                    .get(
                                        &Topic::new_group_message(&v1.group_id[..])
                                            .to_bytes()
                                            .into_vec(),
                                    )
                                    .expect("lease got an unknown topic");
                                slot.got.entry(topic).or_default().push(v1.id);
                            }
                        }
                        Some(LeaseEvent::CatchUpComplete) => slot.catch_ups += 1,
                        Some(LeaseEvent::TopicsLive(_)) => {}
                        Some(LeaseEvent::WelcomeMessages(_)) => {
                            panic!("welcome frames in a group-only model")
                        }
                    }
                }
            }
            if progressed {
                stable = 0;
            } else {
                stable += 1;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }

    /// Wait (bounded) for the transport's reconnect to mint a new session.
    async fn await_session(&mut self) {
        let deadline = tokio::time::Instant::now() + STALL;
        while self.session.is_none() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "transport never reconnected"
            );
            self.adopt_sessions();
            if self.session.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    fn alive_leases(&self) -> Vec<usize> {
        self.leases
            .iter()
            .enumerate()
            .filter(|(_, l)| l.lease.is_some() && !l.dropped)
            .map(|(i, _)| i)
            .collect()
    }

    async fn run_op(&mut self, op: &Op) {
        match op {
            Op::Publish { topic, n } => self.publish(*topic, *n),
            Op::Lease { asks } => {
                if self.alive_leases().len() >= 6 {
                    return;
                }
                // Duplicate topics collapse to the lowest floor asked.
                let mut floors: HashMap<usize, u64> = HashMap::new();
                for (topic, class) in asks {
                    let position = self.position(*topic);
                    let floor = match class {
                        FloorClass::Zero => 0,
                        FloorClass::Mid => position / 2,
                        FloorClass::AtEdge => position,
                        FloorClass::Ahead => position + 1_000,
                    };
                    floors
                        .entry(*topic)
                        .and_modify(|f| *f = (*f).min(floor))
                        .or_insert(floor);
                }
                let subs: Vec<(Topic, u64)> = floors
                    .iter()
                    .map(|(t, f)| (Topic::new_group_message(&gid(*t)[..]), *f))
                    .collect();
                // lease() resolves only once the wire is up and the mutate
                // is accepted; the opener greets new sessions with Started
                // by itself, so this cannot deadlock on the driver.
                let lease = self
                    .transport
                    .lease(subs, DEFAULT_LEASE_DEPTH)
                    .await
                    .expect("lease failed on a healthy model wire");
                self.leases.push(LeaseRef {
                    lease: Some(lease),
                    floors,
                    got: HashMap::new(),
                    catch_ups: 0,
                    dropped: false,
                });
            }
            Op::DropLease { pick } => {
                let alive = self.alive_leases();
                if alive.is_empty() {
                    return;
                }
                let index = alive[*pick as usize % alive.len()];
                self.leases[index].dropped = true;
                self.leases[index].lease = None; // dropping derefs its topics
            }
            Op::ServeWave => self.serve_front(false),
            Op::ServePartial => self.serve_front(true),
            Op::KillWire => {
                if self.kills >= 2 || self.alive_leases().is_empty() {
                    return;
                }
                self.kills += 1;
                // Dropping the session severs both halves; the transport
                // must reconnect (it still holds topics) and re-establish
                // at its own floors.
                self.session = None;
                self.subs.clear();
                self.pending.clear();
                self.await_session().await;
            }
        }
    }

    /// After the schedule: serve every outstanding wave (the model wire is
    /// healthy and stays up — always-ack), then assert the ledger's
    /// delivery contract against the log.
    async fn finish_and_check(&mut self) {
        let deadline = tokio::time::Instant::now() + STALL;
        loop {
            self.settle().await;
            if self.pending.is_empty() {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "pending waves never drained"
            );
            self.serve_front(false);
        }
        self.settle().await;

        for (i, slot) in self.leases.iter().enumerate() {
            for topic in slot.got.keys() {
                assert!(
                    slot.floors.contains_key(topic),
                    "lease {i} received topic {topic}, which it never asked for"
                );
            }
            for (topic, floor) in &slot.floors {
                let got = slot.got.get(topic).map(|v| &v[..]).unwrap_or(&[]);
                // One sweep proves delivery order, exactly-once, and that
                // nothing at-or-below the floor leaked through.
                let mut last = *floor;
                for id in got {
                    assert!(
                        *id > last,
                        "lease {i} delivery on topic {topic} not strictly increasing \
                         above its floor: {id} after {last} (floor {floor})"
                    );
                    last = *id;
                }
                if slot.dropped {
                    continue; // partial receipt is fine; order already checked
                }
                let expected: Vec<u64> = self.log[*topic]
                    .iter()
                    .copied()
                    .filter(|id| *id > *floor)
                    .collect();
                assert_eq!(
                    got,
                    &expected[..],
                    "lease {i} on topic {topic} above floor {floor}: \
                     got != full log suffix"
                );
            }
            if !slot.dropped {
                assert_eq!(
                    slot.catch_ups, 1,
                    "lease {i} saw {} CatchUpCompletes; exactly one owed",
                    slot.catch_ups
                );
            }
        }
    }
}

async fn run_schedule(ops: Vec<Op>) {
    let mut driver = Driver::new();
    for op in &ops {
        driver.run_op(op).await;
        driver.settle().await;
    }
    driver.finish_and_check().await;
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(16),
        ..ProptestConfig::default()
    })]

    #[xmtp_common::test]
    fn ledger_delivers_exactly_the_asked_suffix_in_order(
        ops in proptest::collection::vec(op_strategy(), 4..36)
    ) {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(run_schedule(ops));
    }
}
