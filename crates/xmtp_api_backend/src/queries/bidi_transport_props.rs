//! Model the backend registration protocol over a scripted connection.
//!
//! Each update is acknowledged before its new registrations deliver messages.
//! Targets stay fixed. Removals cancel pending work. Re-adds start a new feed.
//! Generated schedules combine publication, leases, partial delivery, connection
//! failures, suspend, and resume. Both properties require the complete ordered
//! suffix above each lease floor and exactly one completion event per lease.

#![allow(clippy::unwrap_used)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::FutureExt;
use proptest::prelude::*;
use xmtp_proto::backend_v1::SubscribeRequest;

use super::{
    BackendBinding, BidiConnection, BidiTransport, DEFAULT_LEASE_DEPTH, LeaseEvent,
    MAX_MUTATE_BYTES, MAX_MUTATE_TOPICS, OpenError, TopicLease,
};
use xmtp_proto::backend_v1::subscribe_request::Update;
use xmtp_proto::backend_v1::{
    self, CatchupTarget, ServerEnvelope, subscribe_request, subscribe_response,
};
use xmtp_proto::types::Topic;

const N_TOPICS: usize = 3;
const NO_KEEPALIVE: u32 = 3_600_000;
const STALL: Duration = Duration::from_secs(10);

use crate::test::bidi::{MockServer, mock_pair};

type Servers = Arc<Mutex<Vec<MockServer>>>;

fn model_transport(
    chunk_cap: usize,
    chunk_bytes: usize,
) -> (BidiTransport<BackendBinding>, Servers) {
    let servers: Servers = Arc::default();
    let sink = servers.clone();
    let transport = BidiTransport::new_with_chunk_limits(
        move |initial| {
            let (api, server) = mock_pair();
            server.send(subscribe_response::Response::Started(
                subscribe_response::Started {
                    keepalive_interval_ms: NO_KEEPALIVE,
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
        chunk_cap,
        chunk_bytes,
    );
    (transport, servers)
}

fn gid(topic: usize) -> [u8; 16] {
    [topic as u8 + 1; 16]
}

fn group_msg(sequence_id: u64, topic: usize) -> ServerEnvelope {
    ServerEnvelope {
        meta: Some(backend_v1::EnvelopeMeta {
            cursor: Some(backend_v1::Cursor { sequence_id }),
            topic: Some(backend_v1::Topic {
                topic: Topic::new_group_message(gid(topic)).cloned_vec(),
            }),
            ..Default::default()
        }),
        envelope: Some(backend_v1::ClientEnvelope {
            payload: Some(backend_v1::client_envelope::Payload::GroupMessage(
                backend_v1::GroupMessage {
                    data: vec![0xda; 4],
                    ..Default::default()
                },
            )),
        }),
    }
}

fn messages(envelopes: Vec<ServerEnvelope>) -> subscribe_response::Response {
    subscribe_response::Response::Messages(subscribe_response::Messages { envelopes })
}

#[derive(Debug, Clone, Copy)]
enum FloorClass {
    Zero,
    Mid,
    AtEdge,
    Ahead,
}

#[derive(Debug, Clone)]
enum Op {
    Publish { topic: usize, n: u8 },
    Lease { asks: Vec<(usize, FloorClass)> },
    DropLease { pick: u8 },
    ServeTopic,
    ServePartial,
    KillWire,
    Suspend,
    Resume,
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
        3 => Just(Op::ServeTopic),
        1 => Just(Op::ServePartial),
        1 => Just(Op::KillWire),
        1 => Just(Op::Suspend),
        1 => Just(Op::Resume),
    ]
}

struct LeaseRef {
    lease: Option<TopicLease<BackendBinding>>,
    floors: HashMap<usize, u64>,
    got: HashMap<usize, Vec<u64>>,
    catch_ups: usize,
    dropped: bool,
}

struct Driver {
    transport: BidiTransport<BackendBinding>,
    servers: Servers,
    session: Option<MockServer>,
    log: Vec<Vec<u64>>,
    next_cursor: u64,
    subs: HashMap<usize, u64>,
    pending: VecDeque<usize>,
    last_update: u64,
    suspended: bool,
    leases: Vec<LeaseRef>,
    topic_index: HashMap<Vec<u8>, usize>,
    kills: usize,
}

impl Driver {
    fn new(chunk_cap: usize, chunk_bytes: usize) -> Self {
        let (transport, servers) = model_transport(chunk_cap, chunk_bytes);
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
            subs: HashMap::new(),
            pending: VecDeque::new(),
            last_update: 0,
            suspended: false,
            leases: Vec::new(),
            topic_index,
            kills: 0,
        }
    }

    fn position(&self, topic: usize) -> u64 {
        self.log[topic].last().copied().unwrap_or(0)
    }

    fn send(&self, response: subscribe_response::Response) {
        if let Some(session) = &self.session {
            session.send_if_open(response);
        }
    }

    fn on_client_frame(&mut self, frame: SubscribeRequest) {
        match frame.request.expect("client sent empty request") {
            subscribe_request::Request::Update(update) => self.on_update(update),
            other => panic!("unexpected client frame: {other:?}"),
        }
    }

    fn on_update(&mut self, update: Update) {
        assert!(
            update.id > self.last_update,
            "update IDs must increase on one connection"
        );
        self.last_update = update.id;
        let mut unique = HashSet::new();
        for removed in &update.removes {
            assert!(
                unique.insert(removed.topic.clone()),
                "duplicate update topic"
            );
            let topic = self.topic_index[&removed.topic];
            self.subs.remove(&topic);
            self.pending.retain(|pending| *pending != topic);
        }
        let mut targets = Vec::new();
        for sub in update.adds {
            let wire_topic = sub.topic.expect("missing add topic");
            assert!(
                unique.insert(wire_topic.topic.clone()),
                "duplicate or overlapping update topic"
            );
            let topic = self.topic_index[&wire_topic.topic];
            if self.subs.contains_key(&topic) {
                continue;
            }
            let floor = sub.cursor.map_or(0, |cursor| cursor.sequence_id);
            let target = self.position(topic);
            self.subs.insert(topic, floor);
            targets.push(CatchupTarget {
                topic: Some(wire_topic),
                through_sequence_id: target,
            });
            if floor < target {
                self.pending.push_back(topic);
            }
        }
        self.send(subscribe_response::Response::Applied(
            subscribe_response::Applied {
                id: update.id,
                added_targets: targets,
            },
        ));
    }

    fn publish(&mut self, topic: usize, n: u8) {
        for _ in 0..n {
            self.next_cursor += 1;
            let id = self.next_cursor;
            self.log[topic].push(id);
            if self.session.is_some()
                && !self.pending.contains(&topic)
                && let Some(position) = self.subs.get_mut(&topic)
                && id > *position
            {
                *position = id;
                self.send(messages(vec![group_msg(id, topic)]));
            }
        }
    }

    fn serve_front(&mut self, partial: bool) {
        let Some(topic) = self.pending.pop_front() else {
            return;
        };
        let position = self
            .subs
            .get_mut(&topic)
            .expect("pending topic is registered");
        let owed: Vec<_> = self.log[topic]
            .iter()
            .copied()
            .filter(|id| *id > *position)
            .collect();
        let take = if partial { owed.len() / 2 } else { owed.len() };
        let mut frames = Vec::new();
        for id in &owed[..take] {
            *position = *id;
            frames.push(group_msg(*id, topic));
        }
        if take < owed.len() {
            self.pending.push_back(topic);
        }
        for frame in frames {
            self.send(messages(vec![frame]));
        }
    }

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
            self.last_update = 0;
            adopted = true;
        }
        adopted
    }

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
                            slot.lease = None;
                            break;
                        }
                        Some(LeaseEvent::GroupMessages(batch)) => {
                            for message in &batch {
                                let meta = message.meta.as_ref().expect("missing metadata");
                                let topic = self.topic_index
                                    [&meta.topic.as_ref().expect("missing topic").topic];
                                let id = meta.cursor.as_ref().expect("missing cursor").sequence_id;
                                slot.got.entry(topic).or_default().push(id);
                            }
                        }
                        Some(LeaseEvent::CatchUpComplete) => slot.catch_ups += 1,
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
            xmtp_common::time::sleep(Duration::from_millis(1)).await;
        }
    }

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
            xmtp_common::time::sleep(Duration::from_millis(5)).await;
        }
    }

    async fn await_session_if_needed(&mut self) {
        if self.session.is_none() {
            self.await_session().await;
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
            Op::Suspend => {
                self.transport.suspend().await.unwrap();
                self.suspended = true;
                self.session = None;
                self.subs.clear();
                self.pending.clear();
            }
            Op::Resume => {
                self.suspended = false;
                let _reply = self.transport.enqueue_resume().unwrap();
                if !self.alive_leases().is_empty() {
                    self.await_session_if_needed().await;
                }
            }
            Op::ServeTopic => self.serve_front(false),
            Op::ServePartial => self.serve_front(true),
            Op::KillWire => {
                if self.suspended || self.kills >= 2 || self.alive_leases().is_empty() {
                    return;
                }
                self.kills += 1;
                self.session = None;
                self.subs.clear();
                self.pending.clear();
                self.await_session().await;
            }
        }
    }

    async fn finish_and_check(&mut self) {
        if self.suspended {
            self.suspended = false;
            let _reply = self.transport.enqueue_resume().unwrap();
            if !self.alive_leases().is_empty() {
                self.await_session().await;
            }
        }
        let deadline = tokio::time::Instant::now() + STALL;
        loop {
            self.settle().await;
            if self.pending.is_empty() {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "pending topics never drained"
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

async fn run_schedule(ops: Vec<Op>, chunk_cap: usize, chunk_bytes: usize) {
    let mut driver = Driver::new(chunk_cap, chunk_bytes);
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
            .unwrap_or(32),
        ..ProptestConfig::default()
    })]

    #[xmtp_common::test(unwrap_try = true)]
    fn ledger_delivers_exactly_the_asked_suffix_in_order(
        ops in proptest::collection::vec(op_strategy(), 4..36)
    ) {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(run_schedule(ops, MAX_MUTATE_TOPICS, MAX_MUTATE_BYTES));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn chunked_ledger_delivers_exactly_the_asked_suffix_in_order(
        ops in proptest::collection::vec(op_strategy(), 4..36),
        cap in 1usize..=2,
    ) {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(run_schedule(ops, cap, MAX_MUTATE_BYTES));
    }
}
