//! XIP-83 process-level transport: one bidi wire, many topic leases.
//!
//! [`BidiTransport`] owns THE [`Connection`] for a process (or, degenerately, a
//! client) and demuxes raw wire deliveries by topic to [`TopicLease`] holders.
//! It is the boundary between the two layers of the client-integration design:
//! everything here speaks *transport* vocabulary — topics, envelopes, leases,
//! waves — and knows nothing about MLS, databases, decryption, or streams.
//! Decoding and dedup belong to the client-level consumer (the stream router),
//! which is also the durable-cursor owner: the transport never stores a cursor,
//! it only forwards the resume positions a lease hands it.
//!
//! A transport is **not keyed by endpoint** — it has no idea what its opener
//! dials. "One transport per endpoint/environment" is the *caller's*
//! obligation: whoever constructs transports must not hand leases against
//! endpoint A to a transport whose opener connects to endpoint B. The
//! process-level registry that enforces this arrives with client integration.
//!
//! ## Lease semantics
//!
//! - **Every `lease()` is a cursored `Mutate` wave.** The adds are sent even if
//!   another lease already holds the topic: a cursored re-add is the one
//!   catch-up/replay mechanism (XIP-83), and the server replays from the given
//!   cursor. Duplicates that replay causes for other leases are dedup'd by the
//!   consumer, which tracks delivery high-water marks; the transport does not.
//! - **Delivery order is per-topic wire order, which is NOT cursor-monotonic
//!   across a cursored re-add** — that is the replay mechanism working as
//!   designed. In particular, a lease claiming a topic that is already live on
//!   the wire receives in-flight deliveries immediately, possibly *before* the
//!   replay its own wave requested; frames carry no wave tag, so the transport
//!   cannot tell them apart and does not try. Until its `CatchUpComplete`
//!   arrives, a consumer must dedup by exact cursor identity (a seen-set), not
//!   by high-water mark — a high-water mark advanced by an early live delivery
//!   would swallow the replay behind it.
//! - **Removes are refcounted.** A topic leaves the wire only when the *last*
//!   lease holding it derefs. Dropping a [`TopicLease`] derefs its topics.
//! - **The wire is lazy** (opens on the first lease) and closes gracefully
//!   (half-close + bounded drain) when the last lease derefs. A lease arriving
//!   during that drain opens a fresh wire immediately; the closing wire may
//!   briefly overlap it, but its events are discarded, never routed (see
//!   [`close_gracefully`]).
//!
//! ## Backpressure (one policy, every layer)
//!
//! A lease that stops draining its channel is dropped — closed, dereferenced,
//! never blocking the wire or its sibling leases. The dropped consumer's
//! recovery is to re-lease from its durable cursors, which is just another
//! cursored add. This mirrors the consumer-level bounded-channel policy one
//! layer down.
//!
//! ## Not yet here (later phases)
//!
//! - Transparent reconnect: today a dead wire closes every lease (consumers
//!   re-lease to re-open). The reconnect phase moves the re-open inside.
//! - Liveness: a *half-open* wire (the failure probes exist to catch) is not
//!   detected here yet — nothing probes, so leases on such a wire pend until
//!   something above notices. The reconnect phase owns probing + re-open;
//!   until then the consumer keeps its existing watchdog floor.
//! - `Started` capabilities are logged, not consumed (capability gating phase).

use std::collections::{HashMap, HashSet};

use futures::future::BoxFuture;
use tokio::sync::{mpsc, oneshot};
use xmtp_proto::types::Topic;

use super::bidi::{BidiBinding, Connection, Event, TryMutateError};

/// Recommended bound for the transport→lease channel, for callers without a
/// sizing opinion of their own (pass it to [`BidiTransport::lease`]). Consumers
/// are expected to drain fast (they hand off to their own bounded per-stream
/// channels), so this mostly absorbs scheduling jitter, not sustained slowness.
pub const DEFAULT_LEASE_DEPTH: usize = 64;
/// How long the ledger waits before retrying an outbound wave that found the
/// wire's command buffer momentarily full. Rare: capacity normally frees (and
/// the ledger normally wakes) through the same event flow that filled it.
const OUTBOX_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);
/// How long a graceful close (half-close, then drain inbound to completion)
/// may take before the connection is dropped outright. After a half-close the
/// server finishes any in-flight catch-up waves before closing with `OK`
/// (immediately if none are in flight — XIP-83). Here that tail is data
/// nobody leases anymore, so this caps the discard-drain rather than waiting
/// out a deep replay.
const GRACEFUL_CLOSE_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

/// Error type an opener may return; boxed so the transport stays generic over
/// whichever API-client error the integration layer produces.
pub type OpenError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// What the transport needs from a binding beyond the wire vocabulary in
/// [`BidiBinding`]: building a `Mutate` wave from topics + resume cursors, and
/// mapping a delivered message back to its topic (the demux key).
pub trait TransportBinding: BidiBinding
where
    Self::GroupMessage: Clone,
    Self::WelcomeMessage: Clone,
{
    /// A topic's wire resume position (v3: a single id cursor).
    type Cursor: Copy + Send + std::fmt::Debug + 'static;

    /// Build one atomically-applied wave: subscribe each of `adds` from its
    /// cursor, unsubscribe `removes`. `mutate_id` correlates the wave's
    /// `CatchUpComplete` (`0` = no correlation requested).
    ///
    /// Takes iterators by value so a caller that owns its topics pays no clone
    /// — the topic bytes move straight into the wire frame.
    fn build_mutate(
        adds: impl IntoIterator<Item = (Topic, Self::Cursor)>,
        removes: impl IntoIterator<Item = Topic>,
        mutate_id: u64,
    ) -> Self::Mutate;

    /// The topic a delivered group message belongs to (`None` = unroutable).
    fn group_topic(msg: &Self::GroupMessage) -> Option<Topic>;
    /// The topic a delivered welcome belongs to (`None` = unroutable).
    fn welcome_topic(msg: &Self::WelcomeMessage) -> Option<Topic>;
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// The transport is gone (every handle and lease dropped, or its task died).
    #[error("the bidi transport is closed")]
    Closed,
    /// Opening the wire failed; the lease was not registered. Retryable — the
    /// next `lease()` attempts a fresh open.
    #[error("opening the bidi wire failed: {0}")]
    Open(#[source] OpenError),
    /// A lease must name at least one topic: an adds-nothing wave yields no
    /// `CatchUpComplete` (XIP-83), so an empty lease could never resolve its
    /// markers — and it would pin the wire open while receiving nothing.
    #[error("a lease must name at least one topic")]
    Empty,
}

/// Raw wire deliveries for one lease's topics, in wire order. Message payloads
/// are still encrypted wire shapes — decoding is the consumer's job.
pub enum LeaseEvent<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    /// This lease's add-wave is fully caught up to the live edge.
    CatchUpComplete,
    /// The subset of this lease's topics that just crossed to live delivery.
    TopicsLive(Vec<Topic>),
    GroupMessages(Vec<B::GroupMessage>),
    WelcomeMessages(Vec<B::WelcomeMessage>),
}

/// Identifies one lease for the transport's lifetime. Monotonic, never
/// reused, and meaningless outside this module — the wire never sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LeaseId(u64);

/// Correlates a `Mutate` wave with its `CatchUpComplete`: the value that
/// crosses the wire as `mutate_id`. A distinct type from [`LeaseId`] so the
/// ledger's two id spaces cannot be mixed up silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct WaveId(u64);

/// Handle to the process-level transport. Cheap to clone; the underlying
/// ledger task lives until every handle *and* every lease is gone.
pub struct BidiTransport<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    cmds: mpsc::UnboundedSender<Cmd<B>>,
}

// Manual impl: `derive(Clone)` would demand `B: Clone` for no reason.
impl<B: TransportBinding> Clone for BidiTransport<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    fn clone(&self) -> Self {
        Self {
            cmds: self.cmds.clone(),
        }
    }
}

/// A refcounted claim on a set of topics. Receives that subset of the wire's
/// deliveries via [`Self::next`]; dropping it derefs the topics (the last
/// interested lease's deref removes a topic from the wire, and the last lease
/// overall closes the wire).
pub struct TopicLease<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    id: LeaseId,
    topics: Vec<Topic>,
    events: mpsc::Receiver<LeaseEvent<B>>,
    // Deref rides the same unbounded command lane as `lease()`: `Drop` cannot
    // await, and a lost deref would leak a wire subscription forever.
    cmds: mpsc::UnboundedSender<Cmd<B>>,
}

impl<B: TransportBinding> TopicLease<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    /// Next delivery for this lease's topics. `None` means the transport closed
    /// the lease: the wire died, or this consumer fell behind and was dropped
    /// (see module docs) — either way, recover by re-leasing from durable
    /// cursors.
    pub async fn next(&mut self) -> Option<LeaseEvent<B>> {
        self.events.recv().await
    }

    /// The topics this lease holds.
    pub fn topics(&self) -> &[Topic] {
        &self.topics
    }
}

impl<B: TransportBinding> Drop for TopicLease<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    fn drop(&mut self) {
        // The ledger task may already be gone (process teardown) — nothing to
        // deref against then, so a send failure is fine.
        let _ = self.cmds.send(Cmd::Deref(self.id));
    }
}

impl<B: TransportBinding> BidiTransport<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    /// Create a transport that opens its wire lazily via `opener`. The opener
    /// receives the initial `Mutate` (the first lease's cursored adds — the
    /// stream opens with the topic set it will serve) and returns an open
    /// [`Connection`]. Where that connection goes is entirely the opener's
    /// business — see the module docs on endpoint keying.
    pub fn new<O, Fut>(opener: O) -> Self
    where
        O: Fn(B::Mutate) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Connection<B>, OpenError>> + Send + 'static,
    {
        let (cmds, cmds_rx) = mpsc::unbounded_channel();
        let opener: Opener<B> = Box::new(move |initial| Box::pin(opener(initial)));
        // Not detached-and-forgotten: the task exits when `cmds` fully closes,
        // which happens exactly when every handle and lease is dropped.
        xmtp_common::spawn(
            None,
            run_ledger::<B>(opener, cmds_rx, cmds.clone().downgrade()),
        );
        Self { cmds }
    }

    /// Claim `subs` (each topic with the cursor to resume from) with a
    /// transport→lease channel bound of `depth` events (see
    /// [`DEFAULT_LEASE_DEPTH`]).
    ///
    /// Resolves once the lease is registered and its cursored-add wave is
    /// queued for the wire, opening the wire first if this is the first lease.
    /// Waves reach the wire in issue order, but "queued" is not "delivered":
    /// if the wire dies before the wave lands, this lease's event stream ends
    /// (`next()` → `None`) and the consumer re-leases — the same recovery as
    /// any other wire death. A lease whose adds the server has definitely
    /// processed is signaled by its [`LeaseEvent::CatchUpComplete`].
    pub async fn lease(
        &self,
        subs: Vec<(Topic, B::Cursor)>,
        depth: usize,
    ) -> Result<TopicLease<B>, TransportError> {
        if subs.is_empty() {
            // An adds-nothing wave gets no CatchUpComplete (XIP-83), so an
            // empty lease could never resolve — refuse it up front.
            return Err(TransportError::Empty);
        }
        let (reply, response) = oneshot::channel();
        self.cmds
            .send(Cmd::Lease { subs, depth, reply })
            .map_err(|_| TransportError::Closed)?;
        response.await.map_err(|_| TransportError::Closed)?
    }
}

/// Submitted by handles/leases, performed by the ledger task.
enum Cmd<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    Lease {
        subs: Vec<(Topic, B::Cursor)>,
        depth: usize,
        reply: oneshot::Sender<Result<TopicLease<B>, TransportError>>,
    },
    Deref(LeaseId),
}

type Opener<B> = Box<
    dyn Fn(<B as BidiBinding>::Mutate) -> BoxFuture<'static, Result<Connection<B>, OpenError>>
        + Send,
>;

/// The topic ledger: which lease holds which topics, and where each in-flight
/// wave's `CatchUpComplete` should land.
struct Ledger<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    leases: HashMap<LeaseId, LeaseState<B>>,
    by_topic: HashMap<Topic, HashSet<LeaseId>>,
    /// Which lease's `CatchUpComplete` each in-flight wave resolves.
    pending_waves: HashMap<WaveId, LeaseId>,
    next_lease: u64,
    /// Wave correlation ids start at 1: `0` means "no correlation" on the wire.
    next_wave: u64,
}

struct LeaseState<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    tx: mpsc::Sender<LeaseEvent<B>>,
    topics: Vec<Topic>,
}

impl<B: TransportBinding> Default for Ledger<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    fn default() -> Self {
        Self {
            leases: HashMap::new(),
            by_topic: HashMap::new(),
            pending_waves: HashMap::new(),
            next_lease: 0,
            next_wave: 1,
        }
    }
}

impl<B: TransportBinding> Ledger<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    fn next_wave_id(&mut self) -> WaveId {
        let id = WaveId(self.next_wave);
        self.next_wave += 1;
        id
    }

    fn register(&mut self, topics: Vec<Topic>, tx: mpsc::Sender<LeaseEvent<B>>) -> LeaseId {
        let id = LeaseId(self.next_lease);
        self.next_lease += 1;
        for topic in &topics {
            self.by_topic.entry(topic.clone()).or_default().insert(id);
        }
        self.leases.insert(id, LeaseState { tx, topics });
        id
    }

    /// Remove a lease; returns the topics that now have no interested lease
    /// (the ones to remove from the wire). Idempotent — a backpressure drop
    /// followed by the lease's own `Drop`-deref is harmless.
    fn deref(&mut self, id: LeaseId) -> Vec<Topic> {
        let Some(state) = self.leases.remove(&id) else {
            return Vec::new();
        };
        self.pending_waves.retain(|_, lease| *lease != id);
        let mut removes = Vec::new();
        for topic in state.topics {
            if let Some(holders) = self.by_topic.get_mut(&topic) {
                holders.remove(&id);
                if holders.is_empty() {
                    self.by_topic.remove(&topic);
                    removes.push(topic);
                }
            }
        }
        removes
    }

    /// Deliver one event to one lease without blocking the wire. Returns
    /// `false` if the lease must be dropped (wedged past its bound, or its
    /// receiver is already gone).
    fn deliver(&mut self, id: LeaseId, event: LeaseEvent<B>) -> bool {
        let Some(state) = self.leases.get(&id) else {
            return true; // already dropped this routing pass
        };
        match state.tx.try_send(event) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(
                    lease = id.0,
                    "bidi transport: lease wedged past its channel bound; dropping it (re-lease from durable cursors to recover)"
                );
                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }

    /// Route one wire event to the interested leases. Returns the leases that
    /// failed delivery and must be dropped.
    fn route(&mut self, event: Event<B::GroupMessage, B::WelcomeMessage>) -> Vec<LeaseId> {
        let mut dropped = Vec::new();
        match event {
            Event::Started {
                keepalive_interval_ms,
                capabilities,
            } => {
                // Keepalive is consumed by the connection core; capabilities
                // are a later phase (gating/fallback). Nothing to route.
                tracing::debug!(
                    keepalive_interval_ms,
                    ?capabilities,
                    "bidi transport: wire started"
                );
            }
            Event::CatchUpComplete { mutate_id } => {
                if let Some(lease) = self.pending_waves.remove(&WaveId(mutate_id))
                    && !self.deliver(lease, LeaseEvent::CatchUpComplete)
                {
                    dropped.push(lease);
                }
            }
            Event::TopicsLive { topics } => {
                let mut per_lease: HashMap<LeaseId, Vec<Topic>> = HashMap::new();
                for topic in topics {
                    let Some(holders) = self.by_topic.get(&topic) else {
                        continue; // raced a deref — nobody cares anymore
                    };
                    for lease in holders {
                        per_lease.entry(*lease).or_default().push(topic.clone());
                    }
                }
                for (lease, topics) in per_lease {
                    if !self.deliver(lease, LeaseEvent::TopicsLive(topics)) {
                        dropped.push(lease);
                    }
                }
            }
            Event::GroupMessages(batch) => {
                let per_lease = self.demux(batch, B::group_topic, "group");
                for (lease, messages) in per_lease {
                    if !self.deliver(lease, LeaseEvent::GroupMessages(messages)) {
                        dropped.push(lease);
                    }
                }
            }
            Event::WelcomeMessages(batch) => {
                let per_lease = self.demux(batch, B::welcome_topic, "welcome");
                for (lease, messages) in per_lease {
                    if !self.deliver(lease, LeaseEvent::WelcomeMessages(messages)) {
                        dropped.push(lease);
                    }
                }
            }
        }
        dropped
    }

    /// Group a delivery batch by interested lease, preserving wire order within
    /// each lease's slice.
    fn demux<M: Clone>(
        &self,
        batch: Vec<M>,
        topic_of: impl Fn(&M) -> Option<Topic>,
        kind: &'static str,
    ) -> HashMap<LeaseId, Vec<M>> {
        let mut per_lease: HashMap<LeaseId, Vec<M>> = HashMap::new();
        for message in batch {
            let Some(topic) = topic_of(&message) else {
                // A delivery we cannot key by topic cannot be routed. Skip just
                // this message — the rest of the batch is unaffected.
                tracing::warn!("bidi transport: dropping unroutable {kind} message");
                continue;
            };
            let Some(holders) = self.by_topic.get(&topic) else {
                // Deliveries can trail a deref; nobody leases this topic now.
                tracing::debug!(%topic, "bidi transport: delivery for an unleased topic");
                continue;
            };
            for lease in holders {
                per_lease.entry(*lease).or_default().push(message.clone());
            }
        }
        per_lease
    }
}

/// What one iteration of the ledger loop woke up for.
enum Step<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    Cmd(Option<Cmd<B>>),
    Wire(Option<Event<B::GroupMessage, B::WelcomeMessage>>),
    /// Timer backstop fired to retry a parked outbox.
    Retry,
}

/// The ledger task: sole owner of the wire connection and the topic ledger.
///
/// Deadlock discipline: this task is the sole drainer of the wire's events,
/// so it must NEVER await the wire's bounded command channel — the Connection
/// actor parks emitting into a full event channel, and each side would then
/// be blocked on the channel only the other drains (a reachable AB-BA under
/// a delivery flood plus a drop-and-re-lease stampede). All outbound waves
/// therefore go through `outbox` + [`Connection::try_mutate`]: non-blocking,
/// order-preserving, retried opportunistically. The only awaits in this loop
/// are the select (which always keeps draining events) and the opener (which
/// only runs when there is no wire, hence no events to drain).
async fn run_ledger<B: TransportBinding>(
    opener: Opener<B>,
    mut cmds: mpsc::UnboundedReceiver<Cmd<B>>,
    // For minting lease handles. Weak, so the task's own copy never keeps the
    // command channel (and therefore itself) alive.
    lease_cmds: mpsc::WeakUnboundedSender<Cmd<B>>,
) where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    let mut ledger = Ledger::<B>::default();
    let mut conn: Option<Connection<B>> = None;
    // Waves accepted but not yet on the wire, in issue order. Invariant:
    // non-empty only while `conn` is `Some` — cleared on every wire close
    // (stale waves are meaningless to a fresh wire; new leases re-seed it).
    let mut outbox: Outbox<B::Mutate> = Outbox::default();

    loop {
        // Opportunistic flush: capacity frees when the actor makes progress,
        // and the actor's progress is what wakes this loop.
        if let Some(wire) = conn.as_ref() {
            flush_outbox(wire, &mut outbox);
        }

        let step = match conn.as_mut() {
            Some(wire) => tokio::select! {
                cmd = cmds.recv() => Step::Cmd(cmd),
                event = wire.next() => Step::Wire(event),
                // Backstop for the rare quiet-wire case: the command buffer
                // was momentarily full and no event arrives to wake us.
                _ = tokio::time::sleep(OUTBOX_RETRY_INTERVAL), if !outbox.is_empty() => Step::Retry,
            },
            // No wire: nothing to route, just wait for the next command.
            None => Step::Cmd(cmds.recv().await),
        };

        match step {
            // Loop back to the flush at the top.
            Step::Retry => {}
            // Every handle and lease is gone — close up shop.
            Step::Cmd(None) => {
                outbox.clear();
                if let Some(wire) = conn.take() {
                    close_gracefully(wire);
                }
                return;
            }
            Step::Cmd(Some(Cmd::Lease { subs, depth, reply })) => {
                let wave = ledger.next_wave_id();
                let topics: Vec<Topic> = subs.iter().map(|(topic, _)| topic.clone()).collect();
                let mutate = B::build_mutate(subs, None, wave.0);
                let mut queued = None;
                if conn.is_none() {
                    // Lazy open, seeded with this lease's adds. Awaiting here
                    // is safe: with no wire there are no events to drain.
                    match opener(mutate).await {
                        Ok(wire) => conn = Some(wire),
                        Err(e) => {
                            let _ = reply.send(Err(TransportError::Open(e)));
                            continue;
                        }
                    }
                } else {
                    queued = Some(mutate);
                }

                // Registered for routing immediately, NOT at CatchUpComplete:
                // the lease's own replay arrives *before* its CatchUpComplete
                // and must reach it. That in-flight deliveries for an
                // already-live topic also land right away is the documented
                // non-monotonic delivery contract (see module docs) —
                // consumers identity-dedup until they are caught up.
                let (tx, events) = mpsc::channel(depth.max(1));
                let id = ledger.register(topics.clone(), tx);
                ledger.pending_waves.insert(wave, id);
                if let Some(mutate) = queued {
                    // Tagged with the lease so a deref can purge it if it never
                    // reaches the wire.
                    outbox.push(Some(id), mutate);
                }
                let Some(cmds) = lease_cmds.upgrade() else {
                    // Command channel fully closed while we were opening; the
                    // loop will see `None` next and shut down.
                    continue;
                };
                let _ = reply.send(Ok(TopicLease {
                    id,
                    topics,
                    events,
                    cmds,
                }));
            }
            Step::Cmd(Some(Cmd::Deref(id))) => {
                // A wave the dropped lease never got onto the wire must not be
                // sent late: it would replay its topics from a cursor nobody
                // asked for anymore (siblings would receive unrequested
                // backlog, and the wave's completion would route to nobody).
                outbox.purge(id);
                let removes = ledger.deref(id);
                retire(&mut conn, &mut ledger, &mut outbox, removes);
            }
            // The wire ended (server close or transport failure). No transparent
            // reconnect yet (later phase): close every lease so each consumer
            // re-leases from its durable cursors — the next lease re-opens.
            Step::Wire(None) => {
                outbox.clear();
                drop(conn.take());
                close_all_leases(&mut ledger);
            }
            Step::Wire(Some(event)) => {
                let mut removes = Vec::new();
                for lease in ledger.route(event) {
                    removes.extend(ledger.deref(lease));
                }
                retire(&mut conn, &mut ledger, &mut outbox, removes);
            }
        }
    }
}

/// Waves accepted but not yet on the wire, in issue order, each tagged with
/// the lease that issued it (`None` for remove waves, which are never purged).
struct Outbox<M> {
    waves: std::collections::VecDeque<(Option<LeaseId>, M)>,
}

impl<M> Default for Outbox<M> {
    fn default() -> Self {
        Self {
            waves: std::collections::VecDeque::new(),
        }
    }
}

impl<M> Outbox<M> {
    fn push(&mut self, lease: Option<LeaseId>, wave: M) {
        self.waves.push_back((lease, wave));
    }

    fn is_empty(&self) -> bool {
        self.waves.is_empty()
    }

    fn clear(&mut self) {
        self.waves.clear();
    }

    /// Drop every unsent wave issued by `lease` — its deref makes them stale.
    fn purge(&mut self, lease: LeaseId) {
        self.waves.retain(|(owner, _)| *owner != Some(lease));
    }
}

/// Push waves onto the wire until it's momentarily full, closed, or the outbox
/// is empty. Never blocks; `Full` leaves the wave at the front for the next
/// flush, `Closed` is left for the select's `Wire(None)` to clean up.
fn flush_outbox<B: TransportBinding>(wire: &Connection<B>, outbox: &mut Outbox<B::Mutate>)
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    while let Some((lease, wave)) = outbox.waves.pop_front() {
        match wire.try_mutate(wave) {
            Ok(()) => {}
            Err(TryMutateError::Full(wave)) | Err(TryMutateError::Closed(wave)) => {
                outbox.waves.push_front((lease, wave));
                return;
            }
        }
    }
}

/// Take unclaimed topics off the wire; close the wire when no lease is left.
/// Never blocks — the removes wave rides the outbox (flushed at the top of the
/// next loop iteration).
fn retire<B: TransportBinding>(
    conn: &mut Option<Connection<B>>,
    ledger: &mut Ledger<B>,
    outbox: &mut Outbox<B::Mutate>,
    removes: Vec<Topic>,
) where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    if ledger.leases.is_empty() {
        // Unsent waves are for a wire we're closing — nobody needs them.
        outbox.clear();
        if let Some(wire) = conn.take() {
            close_gracefully(wire);
        }
        return;
    }
    if removes.is_empty() || conn.is_none() {
        return;
    }
    outbox.push(None, B::build_mutate(None, removes, 0));
}

fn close_all_leases<B: TransportBinding>(ledger: &mut Ledger<B>)
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    // Dropping the senders ends every lease's event stream (`next()` → None).
    ledger.leases.clear();
    ledger.by_topic.clear();
    ledger.pending_waves.clear();
}

/// Half-close and drain off-task, so the ledger stays responsive; bounded, so
/// the drain task cannot outlive the budget — per XIP-83 the server finishes
/// in-flight catch-up waves before closing, and that tail (all discarded
/// here) can be long. The budget covers `finish()` itself, not just the
/// drain: a parked actor with a full
/// command buffer would otherwise hold the send — and this task, and the
/// connection — forever. On timeout, dropping the wire hard-aborts the actor,
/// which is the correct degraded ending for a wire nobody leases anymore.
///
/// A fresh wire opened by a subsequent lease may briefly overlap this drain.
/// That is deliberate and benign: the closing wire is half-closed (the server
/// finishes in-flight catch-up, then closes its side), its remaining events
/// are discarded here — never routed — and the new wire's cursored adds
/// replay anything a consumer still needs. Blocking new leases on the drain
/// would trade that harmless overlap for up to [`GRACEFUL_CLOSE_BUDGET`] of
/// subscribe latency.
fn close_gracefully<B: TransportBinding>(wire: Connection<B>)
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    xmtp_common::spawn(None, async move {
        let mut wire = wire;
        let _ = tokio::time::timeout(GRACEFUL_CLOSE_BUDGET, async {
            if wire.finish().await.is_err() {
                return; // actor already gone — nothing to drain
            }
            while wire.next().await.is_some() {}
        })
        .await;
    });
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    // Via the queries-level re-exports (the `v3` module itself is private).
    use super::super::{BidiConnection, V3Binding};

    use futures::StreamExt;
    use futures::stream::BoxStream;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use xmtp_proto::api::ApiClientError;
    use xmtp_proto::api_client::XmtpMlsBidiStreams;
    use xmtp_proto::mls_v1::subscribe_request::v1::Mutate;
    use xmtp_proto::mls_v1::{
        GroupMessage, SubscribeRequest, SubscribeResponse, WelcomeMessage, group_message,
        subscribe_request, subscribe_response, welcome_message,
    };
    use xmtp_proto::types::TopicKind;

    const WAIT: Duration = Duration::from_secs(5);

    /// One scripted wire session: captures every frame the client sends and
    /// lets the test play server frames back.
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

    fn mock_pair() -> (MockApi, MockServer) {
        let (to_client, inbound) = tokio::sync::mpsc::unbounded_channel();
        let (captured, from_client) = tokio::sync::mpsc::unbounded_channel();
        (
            MockApi {
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
    impl XmtpMlsBidiStreams for MockApi {
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
                .expect("subscribe_bidi called twice on one mock session");
            Ok(Box::pin(futures::stream::poll_fn(move |cx| {
                inbound.poll_recv(cx)
            })))
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

        async fn next_mutate(&mut self) -> Mutate {
            let frame = tokio::time::timeout(WAIT, self.from_client.recv())
                .await
                .expect("timed out waiting for a client frame")
                .expect("client closed the request stream");
            let Some(subscribe_request::Version::V1(v1)) = frame.version else {
                panic!("client sent unknown request version");
            };
            match v1.request.expect("client sent empty request") {
                subscribe_request::v1::Request::Mutate(mutate) => mutate,
                other => panic!("expected a Mutate, got {other:?}"),
            }
        }

        /// Resolves once the client half-closes its request stream.
        async fn request_stream_ended(&mut self) {
            loop {
                match tokio::time::timeout(WAIT, self.from_client.recv())
                    .await
                    .expect("timed out waiting for the request half-close")
                {
                    Some(_) => continue, // drain trailing frames (e.g. a remove wave)
                    None => return,
                }
            }
        }
    }

    type Servers = Arc<Mutex<Vec<MockServer>>>;

    /// A transport whose opener mints a fresh scripted session per open and
    /// parks the server end where the test can grab it.
    fn transport() -> (BidiTransport<V3Binding>, Servers) {
        let servers: Servers = Arc::default();
        let sink = servers.clone();
        let transport = BidiTransport::new(move |initial| {
            let (api, server) = mock_pair();
            sink.lock().unwrap().push(server);
            async move {
                BidiConnection::open(&api, initial)
                    .await
                    .map_err(|e| Box::new(e) as OpenError)
            }
        });
        (transport, servers)
    }

    fn take_server(servers: &Servers) -> MockServer {
        servers.lock().unwrap().remove(0)
    }

    fn group_topic(id: &[u8]) -> Topic {
        Topic::new_group_message(id)
    }

    fn welcome_topic(installation_key: &[u8]) -> Topic {
        TopicKind::WelcomeMessagesV1.create(installation_key)
    }

    fn group_msg(id: u64, group_id: &[u8]) -> GroupMessage {
        GroupMessage {
            version: Some(group_message::Version::V1(group_message::V1 {
                id,
                group_id: group_id.to_vec(),
                data: vec![0xda; 4],
                ..Default::default()
            })),
        }
    }

    fn welcome_msg(id: u64, installation_key: &[u8]) -> WelcomeMessage {
        WelcomeMessage {
            version: Some(welcome_message::Version::V1(welcome_message::V1 {
                id,
                installation_key: installation_key.to_vec(),
                data: vec![0xef; 4],
                ..Default::default()
            })),
        }
    }

    fn messages(
        group: Vec<GroupMessage>,
        welcome: Vec<WelcomeMessage>,
    ) -> subscribe_response::v1::Response {
        subscribe_response::v1::Response::Messages(subscribe_response::v1::Messages {
            group_messages: group,
            welcome_messages: welcome,
        })
    }

    fn started(keepalive: u32) -> subscribe_response::v1::Response {
        subscribe_response::v1::Response::Started(subscribe_response::v1::Started {
            keepalive_interval_ms: keepalive,
            capabilities: vec![],
        })
    }

    fn catchup_complete(mutate_id: u64) -> subscribe_response::v1::Response {
        subscribe_response::v1::Response::CatchupComplete(subscribe_response::v1::CatchupComplete {
            mutate_id,
        })
    }

    fn topics_live(topics: Vec<&Topic>) -> subscribe_response::v1::Response {
        subscribe_response::v1::Response::TopicsLive(subscribe_response::v1::TopicsLive {
            topics: topics.into_iter().map(Topic::cloned_vec).collect(),
        })
    }

    async fn recv(lease: &mut TopicLease<V3Binding>) -> Option<LeaseEvent<V3Binding>> {
        tokio::time::timeout(WAIT, lease.next())
            .await
            .expect("timed out waiting for a lease event")
    }

    /// Lazy open: no wire before the first lease; the first lease's adds ride
    /// the initial mutate with their cursors and a nonzero wave id.
    #[xmtp_common::test(unwrap_try = true)]
    async fn first_lease_opens_the_wire_with_its_cursored_adds() {
        let (transport, servers) = transport();
        assert!(servers.lock().unwrap().is_empty(), "wire must open lazily");

        let group = group_topic(b"g1");
        let welcome = welcome_topic(b"i1");
        let _lease = transport
            .lease(
                vec![(group.clone(), 5), (welcome.clone(), 0)],
                DEFAULT_LEASE_DEPTH,
            )
            .await?;

        let mut server = take_server(&servers);
        let mutate = server.next_mutate().await;
        assert_eq!(mutate.adds.len(), 2);
        assert_eq!(mutate.adds[0].topic, group.cloned_vec());
        assert_eq!(mutate.adds[0].id_cursor, 5);
        assert_eq!(mutate.adds[1].topic, welcome.cloned_vec());
        assert_eq!(mutate.adds[1].id_cursor, 0);
        assert!(mutate.removes.is_empty());
        assert_ne!(mutate.mutate_id, 0, "waves must be correlatable");
    }

    /// A second lease reuses the open wire, and its adds are sent even for a
    /// topic already on the wire — the cursored re-add IS the catch-up request.
    #[xmtp_common::test(unwrap_try = true)]
    async fn second_lease_is_a_cursored_re_add_on_the_open_wire() {
        let (transport, servers) = transport();
        let shared = group_topic(b"g1");

        let _first = transport.lease(vec![(shared.clone(), 40)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;

        let fresh = group_topic(b"g2");
        let _second = transport
            .lease(vec![(shared.clone(), 7), (fresh.clone(), 0)], 8)
            .await?;
        assert!(
            servers.lock().unwrap().is_empty(),
            "second lease must ride the open wire, not open another"
        );
        let second = server.next_mutate().await;
        assert_eq!(
            second.adds.len(),
            2,
            "the shared topic is re-added with its own cursor"
        );
        assert_eq!(second.adds[0].topic, shared.cloned_vec());
        assert_eq!(second.adds[0].id_cursor, 7);
        assert_ne!(second.mutate_id, first.mutate_id);
    }

    /// Group and welcome deliveries land only on the leases holding their
    /// topic, in wire order; an unroutable message is skipped, not fatal.
    #[xmtp_common::test(unwrap_try = true)]
    async fn deliveries_demux_by_topic() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut beta = transport.lease(vec![(group_topic(b"g2"), 0)], 8).await?;
        let mut inst = transport.lease(vec![(welcome_topic(b"i1"), 0)], 8).await?;
        let server = take_server(&servers);

        let (m1, m2, m3) = (
            group_msg(1, b"g1"),
            group_msg(2, b"g2"),
            group_msg(3, b"g1"),
        );
        let unroutable = GroupMessage { version: None };
        let w = welcome_msg(9, b"i1");
        server.send(messages(
            vec![m1.clone(), unroutable, m2.clone(), m3.clone()],
            vec![w.clone()],
        ));

        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![m1, m3]),
            _ => panic!("alpha expected its two group messages"),
        }
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![m2]),
            _ => panic!("beta expected its one group message"),
        }
        match recv(&mut inst).await {
            Some(LeaseEvent::WelcomeMessages(got)) => assert_eq!(got, vec![w]),
            _ => panic!("inst expected its welcome"),
        }
    }

    /// A topic held by two leases fans its deliveries out to both.
    #[xmtp_common::test(unwrap_try = true)]
    async fn shared_topic_fans_out_to_every_lease() {
        let (transport, servers) = transport();
        let shared = group_topic(b"g1");
        let mut alpha = transport.lease(vec![(shared.clone(), 0)], 8).await?;
        let mut beta = transport.lease(vec![(shared.clone(), 0)], 8).await?;
        let server = take_server(&servers);

        let m = group_msg(1, b"g1");
        server.send(messages(vec![m.clone()], vec![]));

        for lease in [&mut alpha, &mut beta] {
            match recv(lease).await {
                Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![m.clone()]),
                _ => panic!("both leases must receive the shared delivery"),
            }
        }
    }

    /// `CatchUpComplete` lands only on the lease whose wave it echoes, and
    /// `TopicsLive` is filtered to each lease's own topics.
    #[xmtp_common::test(unwrap_try = true)]
    async fn markers_route_to_their_owners() {
        let (transport, servers) = transport();
        let (ga, gb) = (group_topic(b"g1"), group_topic(b"g2"));
        let mut alpha = transport.lease(vec![(ga.clone(), 0)], 8).await?;
        let mut beta = transport.lease(vec![(gb.clone(), 0)], 8).await?;
        let mut server = take_server(&servers);
        let wave_a = server.next_mutate().await.mutate_id;
        let wave_b = server.next_mutate().await.mutate_id;

        server.send(topics_live(vec![&ga, &gb]));
        server.send(catchup_complete(wave_b));
        server.send(catchup_complete(wave_a));

        match recv(&mut alpha).await {
            Some(LeaseEvent::TopicsLive(topics)) => assert_eq!(topics, vec![ga]),
            _ => panic!("alpha expected only its own live topic"),
        }
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        match recv(&mut beta).await {
            Some(LeaseEvent::TopicsLive(topics)) => assert_eq!(topics, vec![gb]),
            _ => panic!("beta expected only its own live topic"),
        }
        // Beta's completion (sent first) must be its next event — proof alpha's
        // completion never crossed over.
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }

    /// Removes are refcounted: dropping one holder of a shared topic removes
    /// only its exclusive topics; dropping the last lease half-closes the wire;
    /// and the next lease after a full close re-opens a fresh one.
    #[xmtp_common::test(unwrap_try = true)]
    async fn deref_is_refcounted_and_last_lease_closes_the_wire() {
        let (transport, servers) = transport();
        let (shared, exclusive) = (group_topic(b"g1"), group_topic(b"g2"));
        let alpha = transport
            .lease(vec![(shared.clone(), 0), (exclusive.clone(), 0)], 8)
            .await?;
        let beta = transport.lease(vec![(shared.clone(), 0)], 8).await?;
        let mut server = take_server(&servers);
        server.next_mutate().await;
        server.next_mutate().await;

        drop(alpha);
        let removal = server.next_mutate().await;
        assert!(removal.adds.is_empty());
        assert_eq!(
            removal.removes,
            vec![exclusive.cloned_vec()],
            "the shared topic still has a holder and must stay"
        );

        drop(beta);
        server.request_stream_ended().await;

        // Graceful, not abortive: after the half-close the connection drains
        // inbound (bounded), so the server's side must still be deliverable. A
        // regression to a hard drop would abort the actor and free the inbound
        // receiver, making this send fail. The half-close above proves `finish`
        // reached the actor before this send, so the check is race-free.
        tokio::time::sleep(Duration::from_millis(50)).await; // let a would-be abort land
        server.send(started(30_000));

        // A lease after a full close opens a fresh wire.
        let _again = transport.lease(vec![(shared, 3)], 8).await?;
        let mut second = take_server(&servers);
        assert_eq!(second.next_mutate().await.adds.len(), 1);
    }

    /// A lease that stops draining is dropped at its channel bound; siblings
    /// keep receiving and the wire never blocks.
    #[xmtp_common::test(unwrap_try = true)]
    async fn slow_lease_is_dropped_without_blocking_siblings() {
        let (transport, servers) = transport();
        let shared = group_topic(b"g1");
        let mut slow = transport.lease(vec![(shared.clone(), 0)], 1).await?;
        let mut fast = transport.lease(vec![(shared.clone(), 0)], 8).await?;
        let server = take_server(&servers);

        let (m1, m2, m3) = (
            group_msg(1, b"g1"),
            group_msg(2, b"g1"),
            group_msg(3, b"g1"),
        );
        server.send(messages(vec![m1.clone()], vec![]));
        server.send(messages(vec![m2.clone()], vec![]));
        server.send(messages(vec![m3.clone()], vec![]));

        // The fast sibling sees everything.
        for expected in [&m1, &m2, &m3] {
            match recv(&mut fast).await {
                Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![expected.clone()]),
                _ => panic!("fast lease must receive every delivery"),
            }
        }
        // The slow one got the one buffered event, then was dropped.
        assert!(matches!(
            recv(&mut slow).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(
            recv(&mut slow).await.is_none(),
            "wedged lease must be closed"
        );
    }

    /// Wire death (no reconnect phase yet) closes every lease; consumers
    /// re-lease from durable cursors, which opens a fresh wire.
    #[xmtp_common::test(unwrap_try = true)]
    async fn wire_death_closes_all_leases_and_a_new_lease_reopens() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let server = take_server(&servers);

        drop(server); // the wire dies
        assert!(
            recv(&mut alpha).await.is_none(),
            "wire death must close the lease"
        );

        let _re = transport.lease(vec![(group_topic(b"g1"), 11)], 8).await?;
        let mut second = take_server(&servers);
        assert_eq!(second.next_mutate().await.adds[0].id_cursor, 11);
    }

    /// A dropped lease's unsent wave is purged from the outbox — it must never
    /// reach the wire late and replay topics nobody asked for. Remove waves
    /// (unowned) and other leases' waves survive, in order.
    #[xmtp_common::test(unwrap_try = true)]
    async fn deref_purges_only_the_dropped_leases_unsent_waves() {
        let mut outbox: Outbox<u32> = Outbox::default();
        outbox.push(Some(LeaseId(1)), 10);
        outbox.push(None, 20); // a remove wave — never purged
        outbox.push(Some(LeaseId(2)), 30);
        outbox.push(Some(LeaseId(1)), 40);
        outbox.purge(LeaseId(1));
        let remaining: Vec<u32> = outbox.waves.iter().map(|(_, wave)| *wave).collect();
        assert_eq!(remaining, vec![20, 30]);
    }

    /// An empty lease is refused before it touches the wire: its adds-nothing
    /// wave would never earn a `CatchUpComplete`, and it would pin the wire
    /// open delivering nothing.
    #[xmtp_common::test(unwrap_try = true)]
    async fn empty_lease_is_refused_without_opening_the_wire() {
        let (transport, servers) = transport();
        let refused = transport.lease(vec![], 8).await;
        assert!(matches!(refused, Err(TransportError::Empty)));
        assert!(servers.lock().unwrap().is_empty());
    }

    #[derive(Debug, thiserror::Error)]
    #[error("no wire for you")]
    struct Refused;

    /// An opener failure surfaces as `TransportError::Open` and registers
    /// nothing — the transport stays usable for a later attempt.
    #[xmtp_common::test(unwrap_try = true)]
    async fn open_failure_surfaces_and_registers_nothing() {
        let transport = BidiTransport::<V3Binding>::new(|_initial| async {
            Err(Box::new(Refused) as OpenError)
        });
        let denied = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await;
        assert!(matches!(denied, Err(TransportError::Open(_))));
        // Still alive: the next attempt reaches the opener again (and fails the
        // same way, proving the ledger task survived the failed open).
        let again = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await;
        assert!(matches!(again, Err(TransportError::Open(_))));
    }
}
