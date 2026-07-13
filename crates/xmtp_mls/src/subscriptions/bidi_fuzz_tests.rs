//! Randomized bidi delivery fuzz against the live containerized node.
//!
//! The scripted transport tests (`xmtp_api_d14n`) prove the client ledger
//! against a *model* of the server; these prove the model — and the pair —
//! against the real thing. A seeded RNG drives random operation schedules
//! (publish bursts, cursored leases at random floors, lower re-adds that
//! yank topics between waves, deliberate unsubscribes, stalled consumers,
//! suspend/resume) at the two layers, and invariant checkers assert the
//! no-loss contract on what actually arrives over the wire:
//!
//! - [`fuzz_server_honors_the_bidi_wave_contract`] drives up to ten raw
//!   [`BidiConnection`]s at once — each with its own checker — and checks
//!   the XIP-83 *server* guarantees frame by frame: every Mutate is acked
//!   (always-ack), a wave's frames precede its `CatchUpComplete` in
//!   per-kind cursor order, no live frame arrives for a wave-owned topic
//!   before that wave completes, and everything above the lowest add
//!   cursor is eventually served — on every connection independently.
//! - [`fuzz_transport_delivery_never_loses_above_the_floor`] drives up to
//!   ten real [`BidiTransport`]s — one per proxied subscriber client, each
//!   on its own faultable wire — and checks the *client* delivery contract
//!   per lease: above its floor, strictly increasing (exactly once, in
//!   cursor order, nothing at-or-below the floor); every lease alive at
//!   the end holds the complete suffix; a lease dropped for backpressure
//!   recovers by re-leasing from what it received (the durable-cursor
//!   recovery shape), and the chain's union is complete.
//!
//! Both layers race the readers against up to ten *producers* — real
//! member clients publishing concurrently into the same groups — so the
//! node's per-topic sequencer and delivery fan-out are exercised under
//! write contention, not just a single well-behaved publisher. Random
//! lease floors are drawn against the GLOBAL publish cursor (`latest`,
//! advanced by every producer across all groups): a mid-history floor is
//! meaningful only because the racing producers keep every group's
//! history moving past it.
//!
//! Reproducibility: the seed is printed at the start and carried in every
//! assertion; replay with `XMTP_BIDI_FUZZ_SEED=<seed>`. The seed replays
//! the operation *schedule* — wire timing still varies run to run, so a
//! failure is a real bug but may take a few replays to re-trigger. Scale
//! the run with `XMTP_BIDI_FUZZ_ROUNDS`. Schedule telemetry (yanks armed,
//! drops recovered, blips, racing bursts, …) is logged at the end of each
//! run: if a lever reads zero across soaks, that path has gone dark —
//! tighten the schedule.
//!
//! Deliberately NOT asserted, and why:
//! - Silence after a remove's ack: the node's delivery fan-out can have
//!   frames in flight when the unsubscribe processes, so removed topics
//!   trail a few live frames. The client drops frames for unheld topics at
//!   demux, so nothing depends on remove promptness.
//! - Cross-topic per-kind cursor order within one wave: the ledger stopped
//!   relying on it (owed-history routing is per topic); per-topic order IS
//!   asserted.
//! - `TopicsLive` content/position and `history_only` Mutates: the former
//!   is informational to this client; the latter is covered by the bounded
//!   catch-up's own live tests (a raw `history_only` re-add of an
//!   already-live topic is server-rejected, which would kill the shared
//!   fuzz connection).
//!
//! Native-only, v3-only, needs the docker backend — like `bidi_tests`.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{RngExt, SeedableRng};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;

use crate::context::XmtpSharedContext;
use crate::tester;
use crate::utils::{LocalTesterBuilder, MlsGroupExt, TesterBuilder};
use xmtp_api_d14n::{
    BidiConnection, BidiEvent, BidiTransport, DEFAULT_LEASE_DEPTH, LeaseEvent, OpenError,
    TopicLease, V3Binding,
};
use xmtp_proto::mls_v1;
use xmtp_proto::mls_v1::subscribe_request::v1::{Mutate, mutate::Subscription};
use xmtp_proto::types::Topic;

/// Wall-clock guard for the settle phases (catch-up completion, sentinel
/// delivery). Generous against a local node; hitting it IS the failure.
const SETTLE: Duration = Duration::from_secs(30);

/// Random publish bursts stop once a group holds this many messages, so the
/// final ground-truth query stays within one page. Racing producers reserve
/// a slot (`fetch_add`) before each send, so the cap holds under
/// contention. Sentinels are exempt.
const PUBLISH_CAP: usize = 60;

fn fuzz_seed() -> u64 {
    match std::env::var("XMTP_BIDI_FUZZ_SEED") {
        Ok(seed) => seed.parse().expect("XMTP_BIDI_FUZZ_SEED must be a u64"),
        Err(_) => rand::rng().random(),
    }
}

fn fuzz_rounds(default: usize) -> usize {
    match std::env::var("XMTP_BIDI_FUZZ_ROUNDS") {
        Ok(rounds) => rounds
            .parse()
            .expect("XMTP_BIDI_FUZZ_ROUNDS must be a usize"),
        Err(_) => default,
    }
}

/// `(topic, cursor)` of a group-message frame; `None` for an undecodable one.
fn gm_parts(m: &mls_v1::GroupMessage) -> Option<(Topic, u64)> {
    use mls_v1::group_message::Version;
    match &m.version {
        Some(Version::V1(v1)) => Some((Topic::new_group_message(&v1.group_id[..]), v1.id)),
        None => None,
    }
}

fn mutate(adds: Vec<(Topic, u64)>, mutate_id: u64) -> Mutate {
    Mutate {
        adds: adds
            .into_iter()
            .map(|(topic, cursor)| Subscription {
                topic: topic.to_bytes().into_vec(),
                id_cursor: cursor,
            })
            .collect(),
        removes: vec![],
        history_only: false,
        mutate_id,
    }
}

/// Cursor of a welcome frame (both variants); `None` if undecodable. Each
/// collector knows the one welcome topic its consumer can lease, so the
/// topic needs no deriving.
fn wm_cursor(m: &mls_v1::WelcomeMessage) -> Option<u64> {
    xmtp_proto::types::WelcomeMessage::try_from(xmtp_api_d14n::v3::V3ProtoWelcomeMessage::from(
        m.clone(),
    ))
    .ok()
    .map(|typed| typed.cursor.sequence_id)
}

fn remove_mutate(topic: Topic, mutate_id: u64) -> Mutate {
    Mutate {
        adds: vec![],
        removes: vec![topic.to_bytes().into_vec()],
        history_only: false,
        mutate_id,
    }
}

/// A random resume floor: the beginning of time, roughly mid-history,
/// exactly the latest cursor, or past the live edge.
fn random_floor(rng: &mut StdRng, latest: u64) -> u64 {
    match rng.random_range(0..4u8) {
        0 => 0,
        1 => latest / 2,
        2 => latest,
        _ => latest + 10_000,
    }
}

/// The server's authoritative per-topic message ids, queried over the
/// regular unary API — the same source of truth the durable path syncs from.
async fn ground_truth<C>(
    api: &C,
    groups: &[(xmtp_proto::types::GroupId, Topic)],
) -> HashMap<Topic, BTreeSet<u64>>
where
    C: xmtp_proto::api_client::XmtpMlsClient,
{
    let mut truth: HashMap<Topic, BTreeSet<u64>> = HashMap::new();
    for (group_id, topic) in groups {
        let messages = api
            .query_group_messages(*group_id)
            .await
            .unwrap_or_else(|_| panic!("ground-truth query failed"));
        let ids = truth.entry(topic.clone()).or_default();
        for message in &messages {
            ids.insert(message.cursor.sequence_id);
        }
        // Every completeness check rests on this query returning the FULL
        // history in one page — fail loudly before the cap creeps past it.
        assert!(
            ids.len() < 95,
            "ground truth is approaching the query page size ({} messages); \
             lower PUBLISH_CAP or add paging",
            ids.len()
        );
    }
    truth
}

/// The server-contract checker's mirror of what WE sent, folded with every
/// frame the server replies — each fold asserts the XIP-83 wire guarantees.
struct ContractState {
    seed: u64,
    /// Waves awaiting their `CatchUpComplete`, with the topics they added.
    unacked: HashMap<u64, Vec<Topic>>,
    acked: HashSet<u64>,
    /// Yanking adds whose live-hold is not yet armed: the guarantee is
    /// ordered on the SERVER's stream, and live frames emitted before the
    /// server processed the Mutate are legal — so the hold arms only at the
    /// wave's first tagged frame (proof the server has processed it; the
    /// stream is ordered, so nothing older can arrive after).
    pending_own: HashMap<u64, HashSet<Topic>>,
    /// Topics inside at least one armed, unacked wave: the live lane must
    /// stay silent for them until every claiming wave completes.
    owned: HashMap<Topic, HashSet<u64>>,
    /// Per-(wave, topic) last replay cursor — a wave's replay is ordered.
    wave_last: HashMap<(u64, Topic), u64>,
    live_last: HashMap<Topic, u64>,
    received: HashMap<Topic, BTreeSet<u64>>,
    latest_seen: u64,
    /// Removes-only waves awaiting their always-ack echo.
    pending_remove: HashMap<u64, Topic>,
    /// Topics removed at any point: exempt from the final completeness
    /// check (their delivery obligation ended mid-run).
    exempt: HashSet<Topic>,
    /// Schedule telemetry: how many yank live-holds actually armed.
    armed_yanks: usize,
}

impl ContractState {
    fn new(seed: u64) -> Self {
        Self {
            seed,
            unacked: HashMap::new(),
            acked: HashSet::new(),
            pending_own: HashMap::new(),
            owned: HashMap::new(),
            wave_last: HashMap::new(),
            live_last: HashMap::new(),
            received: HashMap::new(),
            latest_seen: 0,
            pending_remove: HashMap::new(),
            exempt: HashSet::new(),
            armed_yanks: 0,
        }
    }

    /// Record a wave we sent. `owns` marks adds provably BELOW the topic's
    /// server-side position — only those move the topic into the wave and
    /// hold its live lane (an equal-or-higher re-add is a waveless no-op,
    /// XIP-83). We can only prove "below" against what we've already
    /// received, so the shield check is skipped — never falsely armed — for
    /// adds in the unprovable middle.
    fn sent(&mut self, wave: u64, topic: &Topic, owns: bool) {
        self.unacked.entry(wave).or_default().push(topic.clone());
        if owns {
            self.pending_own
                .entry(wave)
                .or_default()
                .insert(topic.clone());
        }
    }

    /// Record a removes-only wave we sent. The topic's completeness
    /// obligation ends here; the silence obligation arms at the ack.
    fn sent_remove(&mut self, wave: u64, topic: &Topic) {
        self.unacked.entry(wave).or_default();
        self.pending_remove.insert(wave, topic.clone());
        self.exempt.insert(topic.clone());
    }

    /// A strict lower bound on the topic's server-side position: the highest
    /// id we've received for it (positions only grow).
    fn position_bound(&self, topic: &Topic) -> u64 {
        self.received
            .get(topic)
            .and_then(|ids| ids.last())
            .copied()
            .unwrap_or(0)
    }

    fn observe(&mut self, event: BidiEvent) {
        let seed = self.seed;
        match event {
            BidiEvent::Started { .. } | BidiEvent::TopicsLive { .. } => {}
            BidiEvent::CatchUpComplete { mutate_id } => {
                assert_ne!(mutate_id, 0, "untagged CatchUpComplete (seed={seed})");
                assert!(
                    !self.acked.contains(&mutate_id),
                    "wave {mutate_id} acked twice (seed={seed})"
                );
                let topics = self.unacked.remove(&mutate_id).unwrap_or_else(|| {
                    panic!("ack for a wave we never sent: {mutate_id} (seed={seed})")
                });
                self.acked.insert(mutate_id);
                // An empty replay acks without ever arming — clear both.
                self.pending_own.remove(&mutate_id);
                for topic in topics {
                    if let Some(waves) = self.owned.get_mut(&topic) {
                        waves.remove(&mutate_id);
                    }
                }
                self.pending_remove.remove(&mutate_id);
            }
            BidiEvent::GroupMessages {
                messages,
                mutate_id,
            } => {
                // The wave's first tagged frame proves the server processed
                // its Mutate: arm the live-hold for its yanked topics.
                if mutate_id != 0
                    && let Some(topics) = self.pending_own.remove(&mutate_id)
                {
                    for topic in topics {
                        self.owned.entry(topic).or_default().insert(mutate_id);
                        self.armed_yanks += 1;
                    }
                }
                for message in &messages {
                    let Some((topic, id)) = gm_parts(message) else {
                        continue;
                    };
                    // The topic's high-water across BOTH lanes, before this
                    // frame is folded in — the live lane must stay strictly
                    // above it, or a held segment leaked out of its wave.
                    let bound = self.position_bound(&topic);
                    self.received.entry(topic.clone()).or_default().insert(id);
                    self.latest_seen = self.latest_seen.max(id);
                    if mutate_id != 0 {
                        assert!(
                            self.unacked.contains_key(&mutate_id),
                            "frame tagged {mutate_id} outside its wave's lifetime \
                             (unknown or already acked; seed={seed})"
                        );
                        let last = self
                            .wave_last
                            .entry((mutate_id, topic.clone()))
                            .or_insert(0);
                        assert!(
                            id > *last,
                            "wave {mutate_id} replay not cursor-ordered on {topic}: \
                             {id} after {last} (seed={seed})"
                        );
                        *last = id;
                    } else {
                        let owners = self.owned.get(&topic).map(|w| w.len()).unwrap_or(0);
                        assert_eq!(
                            owners, 0,
                            "live frame for wave-owned topic {topic} before its \
                             CatchUpComplete (seed={seed})"
                        );
                        // NOT asserted: silence after a remove's ack. The
                        // node's delivery fan-out can have frames in flight
                        // when the unsubscribe processes, so removed topics
                        // may trail a few live frames. The client is
                        // indifferent — frames for unheld topics are dropped
                        // at demux — so nothing depends on remove promptness.
                        // Strictly above the cross-lane high-water: a live
                        // frame at-or-below it is a held segment leaking out
                        // of its wave — exactly what the client ledger's
                        // live-order shield would silently drop.
                        assert!(
                            id > bound,
                            "live frame {id} at-or-below the high-water {bound} on \
                             {topic} (seed={seed})"
                        );
                        let last = self.live_last.entry(topic.clone()).or_insert(0);
                        assert!(
                            id > *last,
                            "live lane not cursor-ordered on {topic}: {id} after {last} \
                             (seed={seed})"
                        );
                        *last = id;
                    }
                }
            }
            BidiEvent::WelcomeMessages { .. } => {}
        }
    }
}

/// One raw connection under fuzz, with its own checker and wave counter —
/// the server scopes `mutate_id`s per stream, so each connection mints its
/// own and audits its own contract.
struct FuzzConn<C> {
    conn: C,
    state: ContractState,
    next_wave: u64,
    /// Per-topic lowest floor this connection ever asked for — the bound of
    /// its completeness obligation.
    min_added: HashMap<Topic, u64>,
}

/// XIP-83 server-contract fuzz: random subscription churn on up to ten raw
/// connections while up to ten member clients race publishes into the same
/// groups, with every frame checked against the wire guarantees the client
/// ledger is built on — independently per connection.
#[xmtp_common::timeout(Duration::from_secs(300))]
#[xmtp_common::test(unwrap_try = true)]
async fn fuzz_server_honors_the_bidi_wave_contract() {
    let seed = fuzz_seed();
    let rounds = fuzz_rounds(60);
    let mut rng = StdRng::seed_from_u64(seed);

    tester!(alix);
    let n_producers = rng.random_range(2..=10usize);
    let n_conns = rng.random_range(2..=10usize);
    tracing::info!(
        "bidi server-contract fuzz: seed={seed} rounds={rounds} \
         producers={n_producers} conns={n_conns}"
    );
    let mut producers = Vec::new();
    for i in 0..n_producers {
        producers.push(
            TesterBuilder::new()
                .with_name(&format!("wave_prod_{i}"))
                .build()
                .await,
        );
    }
    let member_ids: Vec<_> = producers.iter().map(|p| p.inbox_id()).collect();
    let mut groups = Vec::new();
    for _ in 0..3 {
        let group = alix
            .create_group_with_members(&member_ids, None, None)
            .await
            .expect("create_group_with_members failed");
        let topic = Topic::new_group_message(group.group_id);
        groups.push((group, topic));
    }
    for (group, _) in &groups {
        for msg_idx in 0..rng.random_range(6..14usize) {
            group.send_msg(format!("seed {msg_idx}").as_bytes()).await;
        }
    }
    // Every producer joins every group so racing bursts are real member
    // sends, not creator-only traffic. Each member's FIRST send piggybacks
    // its one-time post-join KeyUpdate commit (PCS rotation) — flush those
    // serially here, so the racing bursts later contend only as app
    // messages at a stable epoch. Join-time epoch churn is MLS-layer
    // behavior with its own tests; this fuzz targets wire delivery.
    let mut producer_groups = Vec::new();
    for producer in &producers {
        producer
            .sync_welcomes()
            .await
            .expect("producer welcome sync failed");
        let mut handles = Vec::new();
        for (group, _) in &groups {
            let handle = producer
                .group(&group.group_id)
                .expect("producer missing a group it was added to");
            handle.send_msg(b"settle").await;
            handles.push(handle);
        }
        producer_groups.push(handles);
    }
    let group_keys: Vec<_> = groups
        .iter()
        .map(|(group, topic)| (group.group_id, topic.clone()))
        .collect();

    // The initial adds use REAL cursors (queried), not just zero, so the
    // nonzero-floor boundary — everything strictly above F — is exercised
    // against the server from the first wave. One pre-open snapshot serves
    // every connection: positions only grow, so `floor < latest_at_query`
    // proves "below the server-side position" at any later open too.
    let api = alix.context.api();
    let mut known_floor: HashMap<Topic, u64> = HashMap::new();
    for (group, topic) in &groups {
        let latest = api
            .query_latest_group_message(group.group_id)
            .await
            .expect("latest query failed")
            .map(|m| m.cursor.sequence_id)
            .unwrap_or(0);
        known_floor.insert(topic.clone(), latest);
    }
    // All connections ride alix's channel: h2 multiplexes them into
    // independent server-side subscriptions, which is the surface under
    // test — per-identity channels would only slow the schedule down.
    let mut conns = Vec::new();
    for _ in 0..n_conns {
        let mut initial: Vec<(Topic, u64)> = Vec::new();
        for (_, topic) in &groups {
            let latest = known_floor[topic];
            let floor = match rng.random_range(0..3u8) {
                0 => 0,
                1 => latest / 2,
                _ => latest.saturating_sub(2),
            };
            initial.push((topic.clone(), floor));
        }
        let conn = BidiConnection::open(&api.api_client, mutate(initial.clone(), 1))
            .await
            .expect("open failed");
        let mut state = ContractState::new(seed);
        for (topic, floor) in &initial {
            // The pre-open query is an authoritative position lower bound.
            state.sent(1, topic, *floor < known_floor[topic]);
        }
        conns.push(FuzzConn {
            conn,
            state,
            next_wave: 2,
            min_added: initial.into_iter().collect(),
        });
    }
    let mut removes_sent = 0usize;
    let mut n_races = 0usize;

    // Racing publish bursts: each task is one member client's burst into
    // one group, running concurrently with every other task and with the
    // subscription churn below.
    let mut bursts: JoinSet<()> = JoinSet::new();
    let caps: Vec<Arc<AtomicUsize>> = groups
        .iter()
        .map(|_| Arc::new(AtomicUsize::new(0)))
        .collect();
    for _ in 0..rounds {
        match rng.random_range(0..10u8) {
            // Spawn racing bursts into a random group from 1-3 producers.
            0..=3 => {
                let g = rng.random_range(0..groups.len());
                let k = rng.random_range(1..=3usize.min(n_producers));
                let mut picks: Vec<usize> = (0..n_producers).collect();
                picks.partial_shuffle(&mut rng, k);
                for p in picks.into_iter().take(k) {
                    let count = rng.random_range(1..5usize);
                    let group = producer_groups[p][g].clone();
                    let cap = caps[g].clone();
                    n_races += 1;
                    bursts.spawn(async move {
                        for i in 0..count {
                            if cap.fetch_add(1, Ordering::Relaxed) >= PUBLISH_CAP {
                                break;
                            }
                            group.send_msg(format!("burst {i}").as_bytes()).await;
                        }
                    });
                }
            }
            // A cursored (re-)add on a random connection — lower re-adds
            // yank topics between waves. Floors are biased around the
            // topic's OWN observed position, so yanks (and their
            // live-holds) actually arm; occasionally two topics ride one
            // wave.
            4..=6 => {
                let c = rng.random_range(0..conns.len());
                let mut picks: Vec<usize> = (0..groups.len()).collect();
                picks.partial_shuffle(&mut rng, 2);
                let n = if rng.random_range(0..3u8) == 0 { 2 } else { 1 };
                let fc = &mut conns[c];
                let wave = fc.next_wave;
                fc.next_wave += 1;
                let mut adds = Vec::new();
                for g in picks.into_iter().take(n) {
                    let topic = &groups[g].1;
                    let bound = fc.state.position_bound(topic);
                    let floor = match rng.random_range(0..5u8) {
                        0 => 0,
                        1 => bound / 2,
                        2 => bound.saturating_sub(3),
                        3 => bound,
                        _ => bound + 10_000,
                    };
                    adds.push((topic.clone(), floor));
                }
                fc.conn
                    .mutate(mutate(adds.clone(), wave))
                    .await
                    .expect("mutate failed");
                for (topic, floor) in adds {
                    let owns = floor < fc.state.position_bound(&topic);
                    fc.state.sent(wave, &topic, owns);
                    let entry = fc.min_added.entry(topic).or_insert(floor);
                    *entry = (*entry).min(floor);
                }
            }
            // A removes-only Mutate on a random connection: always acked
            // (echoing the minted id), and that connection owes the topic
            // silence from the ack until a re-add.
            7 if removes_sent < 3 => {
                let c = rng.random_range(0..conns.len());
                let (_, topic) = &groups[rng.random_range(0..groups.len())];
                let fc = &mut conns[c];
                if !fc.state.exempt.contains(topic)
                    && !fc.state.pending_remove.values().any(|t| t == topic)
                {
                    let wave = fc.next_wave;
                    fc.next_wave += 1;
                    fc.conn
                        .mutate(remove_mutate(topic.clone(), wave))
                        .await
                        .expect("remove mutate failed");
                    fc.state.sent_remove(wave, topic);
                    removes_sent += 1;
                }
            }
            // Drain whatever the server has ready, on every connection —
            // and reap any finished publish bursts.
            _ => {
                while let Some(res) = bursts.try_join_next() {
                    res.expect("publish burst panicked");
                }
                for fc in conns.iter_mut() {
                    loop {
                        match tokio::time::timeout(Duration::from_millis(20), fc.conn.next()).await
                        {
                            Ok(Some(event)) => fc.state.observe(event),
                            Ok(None) => panic!("connection died mid-run (seed={seed})"),
                            Err(_) => break,
                        }
                    }
                }
            }
        }
    }

    // Every racing burst lands before the run is bounded.
    while let Some(res) = bursts.join_next().await {
        res.expect("publish burst panicked");
    }

    // Always-ack: every wave every connection sent must resolve.
    for fc in conns.iter_mut() {
        let deadline = tokio::time::Instant::now() + SETTLE;
        while !fc.state.unacked.is_empty() {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let pending: Vec<u64> = fc.state.unacked.keys().copied().collect();
            assert!(
                !remaining.is_zero(),
                "waves never acked: {pending:?} (seed={seed})"
            );
            match tokio::time::timeout(remaining, fc.conn.next()).await {
                Ok(Some(event)) => fc.state.observe(event),
                Ok(None) => panic!("connection died awaiting acks (seed={seed})"),
                Err(_) => panic!("waves never acked: {pending:?} (seed={seed})"),
            }
        }
    }

    // Sentinels mark the live edge; with every wave acked they arrive on
    // the live lane, and receiving them bounds each connection's
    // completeness check.
    for (group, _) in &groups {
        group.send_msg(b"sentinel").await;
    }
    let truth = ground_truth(&api.api_client, &group_keys).await;
    for fc in conns.iter_mut() {
        let deadline = tokio::time::Instant::now() + SETTLE;
        loop {
            let done = groups.iter().all(|(_, topic)| {
                if fc.state.exempt.contains(topic) {
                    return true; // removed: its sentinel never arrives
                }
                let Some(max) = truth.get(topic).and_then(|ids| ids.last()) else {
                    return true;
                };
                fc.state
                    .received
                    .get(topic)
                    .is_some_and(|ids| ids.contains(max))
            });
            if done {
                break;
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(
                !remaining.is_zero(),
                "sentinels never arrived (seed={seed})"
            );
            match tokio::time::timeout(remaining, fc.conn.next()).await {
                Ok(Some(event)) => fc.state.observe(event),
                Ok(None) => panic!("connection died awaiting sentinels (seed={seed})"),
                Err(_) => panic!("sentinels never arrived (seed={seed})"),
            }
        }
    }

    // No loss, per connection: everything above the lowest cursor it ever
    // asked for was served — by some wave or the live lane. Removed topics
    // are exempt: their delivery obligation ended mid-run.
    for (c, fc) in conns.iter().enumerate() {
        for (topic, min) in &fc.min_added {
            if fc.state.exempt.contains(topic) {
                continue;
            }
            let Some(truth_ids) = truth.get(topic) else {
                continue;
            };
            let got = fc.state.received.get(topic).cloned().unwrap_or_default();
            let missing: Vec<u64> = truth_ids
                .iter()
                .filter(|id| **id > *min && !got.contains(*id))
                .copied()
                .collect();
            assert!(
                missing.is_empty(),
                "server never served {missing:?} on {topic} above cursor {min} \
                 to connection {c} (seed={seed})"
            );
        }
    }
    let waves: u64 = conns.iter().map(|fc| fc.next_wave - 1).sum();
    let armed: usize = conns.iter().map(|fc| fc.state.armed_yanks).sum();
    tracing::info!(
        "bidi server-contract fuzz done: seed={seed} conns={} producers={n_producers} \
         races={n_races} waves={waves} armed_yanks={armed} removes={removes_sent}",
        conns.len(),
    );
}

/// What one lease's collector saw, in arrival order.
#[derive(Default)]
struct Collected {
    ids: HashMap<Topic, Vec<u64>>,
    catch_ups: usize,
    ended: bool,
}

/// One lease in a recovery chain.
struct Link {
    chain: usize,
    /// Index of the consumer (transport) this lease lives on.
    consumer: usize,
    floors: HashMap<Topic, u64>,
    state: Arc<std::sync::Mutex<Collected>>,
    /// Millis the collector sleeps before its next poll — the stall lever
    /// that provokes the transport's backpressure drop.
    stall_ms: Arc<AtomicU64>,
    /// Dropping this unsubscribes deliberately (no recovery obligation).
    die: Option<oneshot::Sender<()>>,
    /// A deliberate unsubscribe, as opposed to a transport-side drop.
    killed: bool,
}

#[allow(clippy::too_many_arguments)]
fn spawn_collector(
    mut lease: TopicLease<V3Binding>,
    state: Arc<std::sync::Mutex<Collected>>,
    stall_ms: Arc<AtomicU64>,
    mut die: oneshot::Receiver<()>,
    ends: mpsc::UnboundedSender<usize>,
    index: usize,
    welcome_topic: Topic,
    latest: Arc<AtomicU64>,
) {
    xmtp_common::spawn(None, async move {
        loop {
            let stall = stall_ms.swap(0, Ordering::Relaxed);
            if stall > 0 {
                xmtp_common::time::sleep(Duration::from_millis(stall)).await;
            }
            tokio::select! {
                _ = &mut die => {
                    state.lock().unwrap().ended = true;
                    return; // dropping the lease derefs its topics
                }
                event = lease.next() => match event {
                    None => {
                        state.lock().unwrap().ended = true;
                        let _ = ends.send(index);
                        return;
                    }
                    Some(LeaseEvent::GroupMessages(batch)) => {
                        let mut state = state.lock().unwrap();
                        for message in &batch {
                            if let Some((topic, id)) = gm_parts(message) {
                                latest.fetch_max(id, Ordering::Relaxed);
                                state.ids.entry(topic).or_default().push(id);
                            }
                        }
                    }
                    Some(LeaseEvent::WelcomeMessages(batch)) => {
                        let mut state = state.lock().unwrap();
                        for message in &batch {
                            if let Some(id) = wm_cursor(message) {
                                state
                                    .ids
                                    .entry(welcome_topic.clone())
                                    .or_default()
                                    .push(id);
                            }
                        }
                    }
                    Some(LeaseEvent::CatchUpComplete) => {
                        state.lock().unwrap().catch_ups += 1;
                    }
                    Some(LeaseEvent::TopicsLive(_)) => {}
                },
            }
        }
    });
}

/// Client delivery-contract fuzz: random lease/yank/drop/stall/suspend
/// schedules — plus real TCP faults, via toxiproxy — on up to ten real
/// [`BidiTransport`]s (one per proxied subscriber client), while up to ten
/// member clients race publishes into the same groups, with per-lease
/// strictly-increasing delivery and chain-union completeness checked
/// against the server's own record.
///
/// Each subscriber's wire runs through its own toxiproxy so a fault op can
/// sever any of them mid-anything (the transport must reconnect
/// transparently and the invariants must still hold); the publishers and
/// the ground-truth query stay on direct connections, so faults never blur
/// what "the server holds" means.
#[xmtp_common::timeout(Duration::from_secs(300))]
#[xmtp_common::test(unwrap_try = true)]
async fn fuzz_transport_delivery_never_loses_above_the_floor() {
    xmtp_common::toxiproxy_test(async || {
        fuzz_transport_delivery(fuzz_seed(), fuzz_rounds(48)).await;
    })
    .await;
}

async fn fuzz_transport_delivery(seed: u64, rounds: usize) {
    let mut rng = StdRng::seed_from_u64(seed);

    tester!(alix);
    let n_producers = rng.random_range(2..=10usize);
    let n_consumers = rng.random_range(2..=10usize);
    tracing::info!(
        "bidi transport fuzz: seed={seed} rounds={rounds} \
         producers={n_producers} consumers={n_consumers}"
    );
    let mut producers = Vec::new();
    for i in 0..n_producers {
        producers.push(
            TesterBuilder::new()
                .with_name(&format!("xport_prod_{i}"))
                .build()
                .await,
        );
    }
    // Consumers read raw frames off leased topics — no MLS membership
    // needed — but each one is a real proxied client with its own faultable
    // wire, its own transport, and its own welcome topic.
    let mut consumers = Vec::new();
    for i in 0..n_consumers {
        consumers.push(
            TesterBuilder::new()
                .with_name(&format!("xport_sub_{i}"))
                .proxy()
                .build()
                .await,
        );
    }
    let member_ids: Vec<_> = producers.iter().map(|p| p.inbox_id()).collect();
    let initial_groups = 4usize;
    let mut groups = Vec::new();
    for _ in 0..initial_groups {
        let group = alix
            .create_group_with_members(&member_ids, None, None)
            .await
            .expect("create_group_with_members failed");
        let topic = Topic::new_group_message(group.group_id);
        groups.push((group, topic));
    }
    for (group, _) in &groups {
        for msg_idx in 0..rng.random_range(6..14usize) {
            group.send_msg(format!("seed {msg_idx}").as_bytes()).await;
        }
    }
    // As in the server-contract fuzz: settle each producer's one-time
    // post-join KeyUpdate commit serially, so racing bursts hit a stable
    // epoch.
    let mut producer_groups = Vec::new();
    for producer in &producers {
        producer
            .sync_welcomes()
            .await
            .expect("producer welcome sync failed");
        let mut handles = Vec::new();
        for (group, _) in &groups {
            let handle = producer
                .group(&group.group_id)
                .expect("producer missing a group it was added to");
            handle.send_msg(b"settle").await;
            handles.push(handle);
        }
        producer_groups.push(handles);
    }

    let api = alix.context.api();
    let mut transports: Vec<BidiTransport<V3Binding>> = Vec::new();
    let mut welcome_topics = Vec::new();
    for consumer in &consumers {
        let api = consumer.context.api().api_client.clone();
        transports.push(BidiTransport::new(move |initial| {
            let api = api.clone();
            async move {
                BidiConnection::open(&api, initial)
                    .await
                    .map_err(OpenError::new)
            }
        }));
        // Each consumer's welcome topic is fuzzed like any other: its
        // leases may include it, and a fuzz op mints real welcomes by
        // creating groups WITH random consumers. Its own id space, so its
        // own floor palette.
        welcome_topics.push(Topic::new_welcome_message(
            consumer.context.installation_id(),
        ));
    }

    let (ends_tx, mut ends_rx) = mpsc::unbounded_channel::<usize>();
    let mut links: Vec<Link> = Vec::new();
    // chain id -> the chain's ORIGINAL floors (the durable ask the whole
    // recovery chain must eventually satisfy). The consumer a chain lives
    // on rides along in its links.
    let mut chains: HashMap<usize, HashMap<Topic, u64>> = HashMap::new();
    // The REAL group-message high-water, folded by every collector — random
    // floors derive from it, so mid-history and at-the-edge floors actually
    // mean what they say (a fabricated estimate would collapse them all to
    // "beginning of time" against the shared node's global sequence).
    let latest = Arc::new(AtomicU64::new(0));
    let mut suspended = vec![false; n_consumers];
    let mut resume_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    let mut bursts: JoinSet<()> = JoinSet::new();
    let mut caps: Vec<Arc<AtomicUsize>> = groups
        .iter()
        .map(|_| Arc::new(AtomicUsize::new(0)))
        .collect();
    let mut welcome_groups = 0usize;
    let mut welcomes_created = 0usize;
    // Schedule telemetry, reported at the end: proof the paths actually ran.
    let (mut n_kills, mut n_stalls, mut n_blips, mut n_suspends, mut n_relinks, mut n_races) =
        (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);

    // Lease `floors` as a new link of `chain`, on `consumer`'s transport.
    macro_rules! add_link {
        ($chain:expr, $floors:expr, $consumer:expr) => {{
            let consumer: usize = $consumer;
            let floors: HashMap<Topic, u64> = $floors;
            let subs: Vec<(Topic, u64)> = floors.iter().map(|(t, c)| (t.clone(), *c)).collect();
            let depth = if rng.random_range(0..3u8) == 0 {
                2
            } else {
                DEFAULT_LEASE_DEPTH
            };
            // A subscribe during a network fault can fail its cold open —
            // real consumers re-subscribe, so the fuzz does too. 40 tries
            // at 250ms is a 10s budget: comfortably past the longest
            // scheduled blip plus reconnect backoff, while still failing
            // fast on a genuinely wedged transport.
            let mut attempts = 0;
            let lease = loop {
                match transports[consumer].lease(subs.clone(), depth).await {
                    Ok(lease) => break lease,
                    Err(e) => {
                        attempts += 1;
                        assert!(attempts < 40, "lease kept failing: {e} (seed={seed})");
                        xmtp_common::time::sleep(Duration::from_millis(250)).await;
                    }
                }
            };
            let state = Arc::new(std::sync::Mutex::new(Collected::default()));
            let stall_ms = Arc::new(AtomicU64::new(0));
            let (die_tx, die_rx) = oneshot::channel();
            let index = links.len();
            spawn_collector(
                lease,
                state.clone(),
                stall_ms.clone(),
                die_rx,
                ends_tx.clone(),
                index,
                welcome_topics[consumer].clone(),
                latest.clone(),
            );
            links.push(Link {
                chain: $chain,
                consumer,
                floors,
                state,
                stall_ms,
                die: Some(die_tx),
                killed: false,
            });
        }};
    }

    // A transport-side drop (backpressure) recovers like a durable-cursor
    // re-subscribe: a new link from what the chain has received so far, on
    // the same consumer.
    macro_rules! relink {
        ($index:expr) => {{
            let index: usize = $index;
            if !links[index].killed {
                let chain = links[index].chain;
                let consumer = links[index].consumer;
                let mut floors = chains[&chain].clone();
                for link in links.iter().filter(|l| l.chain == chain) {
                    let state = link.state.lock().unwrap();
                    for (topic, ids) in &state.ids {
                        if let (Some(floor), Some(max)) = (floors.get_mut(topic), ids.iter().max())
                        {
                            *floor = (*floor).max(*max);
                        }
                    }
                }
                add_link!(chain, floors, consumer);
                n_relinks += 1;
            }
        }};
    }

    for _ in 0..rounds {
        while let Ok(index) = ends_rx.try_recv() {
            relink!(index);
        }
        while let Some(res) = bursts.try_join_next() {
            res.expect("publish burst panicked");
        }
        match rng.random_range(0..20u8) {
            // Racing publish bursts: 1-3 member producers into one group,
            // concurrent with each other and with everything below.
            // Mid-run (welcome-op) groups have no producer members, so
            // alix carries those — the long-lived groups get the races.
            0..=5 => {
                let g = rng.random_range(0..groups.len());
                if g < initial_groups {
                    let k = rng.random_range(1..=3usize.min(n_producers));
                    let mut picks: Vec<usize> = (0..n_producers).collect();
                    picks.partial_shuffle(&mut rng, k);
                    for p in picks.into_iter().take(k) {
                        let count = rng.random_range(1..6usize);
                        let group = producer_groups[p][g].clone();
                        let cap = caps[g].clone();
                        n_races += 1;
                        bursts.spawn(async move {
                            for i in 0..count {
                                if cap.fetch_add(1, Ordering::Relaxed) >= PUBLISH_CAP {
                                    break;
                                }
                                group.send_msg(format!("burst {i}").as_bytes()).await;
                            }
                        });
                    }
                } else {
                    let count = rng.random_range(1..6usize);
                    let group = groups[g].0.clone();
                    let cap = caps[g].clone();
                    bursts.spawn(async move {
                        for i in 0..count {
                            if cap.fetch_add(1, Ordering::Relaxed) >= PUBLISH_CAP {
                                break;
                            }
                            group.send_msg(format!("burst {i}").as_bytes()).await;
                        }
                    });
                }
            }
            // A new lease (its own chain) on a random consumer: 1-3
            // distinct random topics at random floors — shared topics at
            // lower floors are the yank engine — sometimes including that
            // consumer's welcome topic.
            6..=10 => {
                let consumer = rng.random_range(0..n_consumers);
                let mut floors = HashMap::new();
                let mut picks: Vec<usize> = (0..groups.len()).collect();
                let n = rng.random_range(1..4usize).min(groups.len());
                picks.partial_shuffle(&mut rng, n);
                for g in picks.into_iter().take(n) {
                    let topic = groups[g].1.clone();
                    let floor = random_floor(&mut rng, latest.load(Ordering::Relaxed));
                    floors.insert(topic, floor);
                }
                if rng.random_range(0..3u8) == 0 {
                    // Welcome ids are their own sequence: fuzz the two
                    // behavior classes (full replay vs owed-nothing).
                    let floor = if rng.random_range(0..4u8) == 0 {
                        u64::MAX / 2
                    } else {
                        0
                    };
                    floors.insert(welcome_topics[consumer].clone(), floor);
                }
                let chain = chains.len();
                chains.insert(chain, floors.clone());
                add_link!(chain, floors, consumer);
            }
            // Deliberate unsubscribe: no recovery obligation.
            11..=12 => {
                let alive: Vec<usize> = links
                    .iter()
                    .enumerate()
                    .filter(|(_, l)| l.die.is_some() && !l.state.lock().unwrap().ended)
                    .map(|(i, _)| i)
                    .collect();
                if !alive.is_empty() {
                    let index = alive[rng.random_range(0..alive.len())];
                    links[index].killed = true;
                    drop(links[index].die.take());
                    n_kills += 1;
                }
            }
            // Stall a consumer — with a shallow channel this provokes the
            // transport's backpressure drop and the recovery chain.
            13..=14 => {
                if !links.is_empty() {
                    let index = rng.random_range(0..links.len());
                    links[index]
                        .stall_ms
                        .store(rng.random_range(400..1500), Ordering::Relaxed);
                    n_stalls += 1;
                }
            }
            // Sever a random consumer's TCP mid-anything — occasionally
            // all of them at once (a correlated outage), occasionally long
            // enough to eat several reconnect attempts. Every affected
            // transport must reconnect transparently and re-serve whatever
            // the cut ate.
            15 => {
                let blip = if rng.random_range(0..4u8) == 0 {
                    Duration::from_millis(rng.random_range(1200..2400))
                } else {
                    Duration::from_millis(rng.random_range(150..600))
                };
                let targets: Vec<usize> = if rng.random_range(0..4u8) == 0 {
                    (0..n_consumers).collect()
                } else {
                    vec![rng.random_range(0..n_consumers)]
                };
                for &c in &targets {
                    consumers[c]
                        .for_each_proxy(async |p| {
                            p.disable().await.unwrap();
                        })
                        .await;
                }
                xmtp_common::time::sleep(blip).await;
                for &c in &targets {
                    consumers[c]
                        .for_each_proxy(async |p| {
                            p.enable().await.unwrap();
                        })
                        .await;
                }
                n_blips += 1;
            }
            // The app-lifecycle pair, per consumer.
            16..=17 => {
                let c = rng.random_range(0..n_consumers);
                if suspended[c] {
                    let transport = transports[c].clone();
                    resume_handles.push(tokio::spawn(async move {
                        transport.resume().await.expect("resume failed");
                    }));
                    suspended[c] = false;
                } else {
                    transports[c].suspend().await.expect("suspend failed");
                    suspended[c] = true;
                    n_suspends += 1;
                }
            }
            // Real welcomes for 1-2 random consumers: a new group created
            // with them. The group joins the fuzz set too — a zero-history
            // topic born after the wires opened.
            18 if welcome_groups < 12 => {
                let k = rng.random_range(1..=2usize.min(n_consumers));
                let mut picks: Vec<usize> = (0..n_consumers).collect();
                picks.partial_shuffle(&mut rng, k);
                let members: Vec<_> = picks[..k]
                    .iter()
                    .map(|&c| consumers[c].inbox_id())
                    .collect();
                let group = alix
                    .create_group_with_members(&members, None, None)
                    .await
                    .expect("create_group_with_members failed");
                let topic = Topic::new_group_message(group.group_id);
                groups.push((group, topic));
                caps.push(Arc::new(AtomicUsize::new(0)));
                welcome_groups += 1;
                welcomes_created += k;
            }
            // Let the wire breathe.
            _ => xmtp_common::time::sleep(Duration::from_millis(rng.random_range(10..80))).await,
        }
        xmtp_common::time::sleep(Duration::from_millis(rng.random_range(5..30))).await;
    }

    // Wind down: back on the network, every racing burst landed, recover
    // any still-pending drops, and wait out every resume() (each resolves
    // at a caught-up live wire).
    for (c, transport) in transports.iter().enumerate() {
        if suspended[c] {
            let transport = transport.clone();
            resume_handles.push(tokio::spawn(async move {
                transport.resume().await.expect("resume failed");
            }));
        }
    }
    while let Some(res) = bursts.join_next().await {
        res.expect("publish burst panicked");
    }
    for handle in resume_handles {
        tokio::time::timeout(SETTLE, handle)
            .await
            .unwrap_or_else(|_| panic!("resume() never resolved (seed={seed})"))
            .expect("resume task panicked");
    }
    while let Ok(index) = ends_rx.try_recv() {
        relink!(index);
    }

    // Sentinels bound the run; ground truth is the server's own record.
    // The welcome sentinel is one more group created with EVERY consumer,
    // so each welcome topic gets a live edge to confirm.
    for (group, _) in &groups {
        group.send_msg(b"sentinel").await;
    }
    {
        let members: Vec<_> = consumers.iter().map(|c| c.inbox_id()).collect();
        alix.create_group_with_members(&members, None, None)
            .await
            .expect("welcome sentinel failed");
    }
    // Computed here, not at setup: the fuzz set grows with the groups the
    // welcome op created mid-run.
    let group_keys: Vec<_> = groups
        .iter()
        .map(|(group, topic)| (group.group_id, topic.clone()))
        .collect();
    let mut truth = ground_truth(&api.api_client, &group_keys).await;
    for (c, consumer) in consumers.iter().enumerate() {
        let welcome_truth: BTreeSet<u64> = api
            .query_welcome_messages(consumer.context.installation_id())
            .await
            .expect("welcome ground-truth query failed")
            .iter()
            .map(|w| w.cursor.sequence_id)
            .collect();
        truth.insert(welcome_topics[c].clone(), welcome_truth);
    }
    let max_id = |topic: &Topic| truth.get(topic).and_then(|ids| ids.last()).copied();

    // Every surviving lease must reach its catch-up marker AND the live
    // edge of each topic it asked for. Hitting the deadline is the loss
    // (or hang) detector firing.
    let deadline = tokio::time::Instant::now() + SETTLE;
    loop {
        // A drop landing during the wind-down (residual stall, sentinel
        // burst) still recovers here — otherwise its whole chain would end
        // unchecked and a loss could pass silently.
        while let Ok(index) = ends_rx.try_recv() {
            relink!(index);
        }
        let lagging: Vec<String> = links
            .iter()
            .enumerate()
            .filter(|(_, link)| !link.killed && !link.state.lock().unwrap().ended)
            .flat_map(|(i, link)| {
                let state = link.state.lock().unwrap();
                let mut lags = Vec::new();
                if state.catch_ups == 0 {
                    lags.push(format!("link {i}: no CatchUpComplete"));
                }
                for (topic, floor) in &link.floors {
                    let Some(max) = max_id(topic) else { continue };
                    if max <= *floor {
                        continue; // subscribed past the sentinel — owed nothing
                    }
                    let got = state.ids.get(topic).is_some_and(|ids| ids.contains(&max));
                    if !got {
                        lags.push(format!("link {i}: missing sentinel {max} on {topic}"));
                    }
                }
                lags
            })
            .collect();
        if lagging.is_empty() {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "leases never converged (seed={seed}): {lagging:?}"
        );
        xmtp_common::time::sleep(Duration::from_millis(250)).await;
    }

    let n_drops = links
        .iter()
        .filter(|l| !l.killed && l.state.lock().unwrap().ended)
        .count();
    tracing::info!(
        "bidi transport fuzz schedule: seed={seed} producers={n_producers} \
         consumers={n_consumers} links={} drops={n_drops} relinks={n_relinks} \
         kills={n_kills} stalls={n_stalls} blips={n_blips} suspends={n_suspends} \
         races={n_races} welcomes={welcomes_created}",
        links.len(),
    );

    // Per-link: strictly increasing above its own floor — one sweep proves
    // delivery order, exactly-once, and that nothing at-or-below the floor
    // leaked through (both demux lanes honor the holder's floor; only
    // cursor-less frames fail open, and those never reach a collector).
    for (i, link) in links.iter().enumerate() {
        let state = link.state.lock().unwrap();
        assert!(
            state.catch_ups <= 1,
            "link {i} caught up {} times (seed={seed})",
            state.catch_ups
        );
        for topic in state.ids.keys() {
            assert!(
                link.floors.contains_key(topic),
                "link {i} received {topic}, which it never leased (seed={seed})"
            );
        }
        for (topic, floor) in &link.floors {
            let ids: &[u64] = state.ids.get(topic).map(|v| &v[..]).unwrap_or(&[]);
            let mut last = *floor;
            for id in ids {
                assert!(
                    *id > last,
                    "link {i} delivery on {topic} not strictly increasing above \
                     the floor: {id} after {last} (floor {floor}, seed={seed})"
                );
                last = *id;
            }
            // A link alive at the end holds the complete suffix.
            if !state.ended
                && let Some(truth_ids) = truth.get(topic)
            {
                let got: BTreeSet<u64> = ids.iter().copied().collect();
                let missing: Vec<u64> = truth_ids
                    .iter()
                    .filter(|id| **id > *floor && !got.contains(*id))
                    .copied()
                    .collect();
                assert!(
                    missing.is_empty(),
                    "link {i} lost {missing:?} on {topic} above floor {floor} (seed={seed})"
                );
            }
        }
    }

    // Per-chain: the union across recovery links covers everything above
    // the ORIGINAL floors — the durable-cursor recovery contract. Only a
    // chain the schedule deliberately unsubscribed is exempt; a chain whose
    // last link ended AFTER convergence already holds the sentinel, so
    // `ended` there does not weaken the union.
    for (chain, floors) in &chains {
        let deliberately_ended = links
            .iter()
            .filter(|l| l.chain == *chain)
            .next_back()
            .is_some_and(|l| l.killed);
        if deliberately_ended {
            continue;
        }
        let mut union: HashMap<Topic, BTreeSet<u64>> = HashMap::new();
        for link in links.iter().filter(|l| l.chain == *chain) {
            let state = link.state.lock().unwrap();
            for (topic, ids) in &state.ids {
                union.entry(topic.clone()).or_default().extend(ids);
            }
        }
        for (topic, floor) in floors {
            let Some(truth_ids) = truth.get(topic) else {
                continue;
            };
            let got = union.get(topic).cloned().unwrap_or_default();
            let missing: Vec<u64> = truth_ids
                .iter()
                .filter(|id| **id > *floor && !got.contains(*id))
                .copied()
                .collect();
            assert!(
                missing.is_empty(),
                "chain {chain} lost {missing:?} on {topic} above floor {floor} (seed={seed})"
            );
        }
    }
}
