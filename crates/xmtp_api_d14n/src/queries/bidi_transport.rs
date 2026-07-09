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
//!   cursor.
//! - **The delivery guarantee.** The ledger keeps per-lease per-topic delivery
//!   positions (seeded at the lease's add cursors). During a lease's catch-up
//!   window (subscribe → its `CatchUpComplete`) it admits anything above the
//!   topic's *frozen floor* — never filtering by the advancing position, or an
//!   early live delivery would swallow the replay behind it. Once caught up, a
//!   lease receives each trackable message **at most once**: a sibling's
//!   deeper re-add replays all it wants, the copies are dropped here. What
//!   this cannot cover: identity duplicates *inside* the window (overlapping
//!   concurrent replays are untagged on the wire — the consumer keeps an
//!   exact-identity seen-set until its `CatchUpComplete`), history the client
//!   already holds in storage (a resuming consumer seeds its own dedup from
//!   its store), and messages with no extractable cursor (fail open).
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
//! ## Reconnect (a wire flap is invisible to leases)
//!
//! A dead wire does not close leases. The transport re-opens on a capped
//! exponential backoff — forever; an offline process resumes when the network
//! returns — and sends one **resume wave** re-adding every leased topic from
//! its last-seen delivery position, held down to the floor of any lease still
//! owed its catch-up replay. That wave's `CatchUpComplete` resolves every
//! lease that was still catching up when the wire died. Replay overlap is
//! absorbed by the delivery guarantee above, so consumers never observe the
//! flap. `next()` → `None` now means only: this consumer fell behind and was
//! dropped, or the transport itself shut down.
//!
//! ## Suspend / resume (the app-lifecycle touchpoint)
//!
//! [`BidiTransport::suspend`] half-closes the wire and parks: leases and wire
//! positions are kept, new leases register without touching the network, and
//! nothing reconnects until [`BidiTransport::resume`] — which re-opens with
//! the same resume wave a wire flap uses and resolves at that wave's
//! `CatchUpComplete`: "catch up, then done", the background-fetch primitive.
//! An unpaired `resume()` (live wire, or nothing leased) resolves
//! immediately; a process thawed *without* suspending doesn't need one — the
//! connection's watchdog detects the stale wire and the reconnect above
//! absorbs it.
//!
//! ## Liveness
//!
//! A *half-open* wire needs no handling here: the connection actor's silence
//! watchdog (see `bidi.rs`) tears down a wire with no inbound frames for the
//! probe budget, which lands in the same wire-death path as any other close —
//! the reconnect above absorbs it. The transport never probes inline; awaiting
//! a probe from the ledger loop would stop draining events, park the actor on
//! a full event channel, and starve the very pong it waits for.
//!
//! ## Not yet here (later phases)
//!
//! - `Started` capabilities are logged, not consumed (capability gating phase).

use std::collections::{HashMap, HashSet};

use tokio::sync::{mpsc, oneshot};
use xmtp_common::{BoxDynFuture, MaybeSend, MaybeSync};
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
/// First retry after an unexpected wire death; doubles per failed attempt up
/// to [`RECONNECT_MAX_DELAY`]. Reconnection is never given up on — an offline
/// process resumes when the network returns.
const RECONNECT_INITIAL_DELAY: std::time::Duration = std::time::Duration::from_millis(100);
/// Ceiling for the reconnect backoff.
const RECONNECT_MAX_DELAY: std::time::Duration = std::time::Duration::from_secs(30);
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

    /// The wire resume position a delivered group message represents — feeds
    /// the per-lease delivery positions behind the module's delivery
    /// guarantee. `None` = untrackable; such a message fans out unfiltered
    /// and the consumer's own dedup absorbs it.
    fn group_cursor(msg: &Self::GroupMessage) -> Option<Self::Cursor>;
    /// Welcome analog of [`Self::group_cursor`].
    fn welcome_cursor(msg: &Self::WelcomeMessage) -> Option<Self::Cursor>;
    /// Fold a delivered position into a tracked one (v3: `max`; d14n:
    /// per-originator max-merge).
    fn advance(position: &mut Self::Cursor, delivered: Self::Cursor);
    /// Whether `position` already covers `delivered` (v3: `delivered <=
    /// position`).
    fn covers(position: &Self::Cursor, delivered: &Self::Cursor) -> bool;
    /// The greatest position covered by both (v3: `min`) — folds multiple
    /// resume constraints for one topic into a single safe re-add cursor
    /// (replaying too much is dedup'd; replaying too little loses messages).
    fn meet(a: Self::Cursor, b: Self::Cursor) -> Self::Cursor;
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
        O: Fn(B::Mutate) -> Fut + MaybeSend + MaybeSync + 'static,
        Fut: Future<Output = Result<Connection<B>, OpenError>> + MaybeSend + 'static,
    {
        let (cmds, cmds_rx) = mpsc::unbounded_channel();
        let opener: Opener<B> = Box::new(
            move |initial| -> BoxDynFuture<'static, Result<Connection<B>, OpenError>> {
                Box::pin(opener(initial))
            },
        );
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
    /// queued for the wire, opening the wire first if this is the first lease
    /// (unless suspended — then the lease registers and waits for
    /// [`Self::resume`]). Waves reach the wire in issue order. A wire death at
    /// any point is invisible here: the transport reconnects and a resume wave
    /// re-covers this lease (module docs), so `next()` → `None` means only
    /// that this consumer fell behind and was dropped, or the transport shut
    /// down. A lease whose adds the server has definitely processed is
    /// signaled by its [`LeaseEvent::CatchUpComplete`].
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

    /// Take the transport off the network — the app-backgrounding half of the
    /// lifecycle pair. The wire goes down as a graceful half-close; every
    /// lease and wire position is kept, new leases register without touching
    /// the network, and nothing reconnects until [`Self::resume`]. Idempotent,
    /// and a no-op on a wire that is already down (or was never opened).
    ///
    /// Resolves once the transport has **released** the wire: no new work
    /// will touch it, and a bounded, discard-only half-close drain finishes
    /// in the background (deliberately not awaited — see the ledger's
    /// deadlock discipline). A `resume()` right after may briefly overlap
    /// that dying drain with the fresh wire; the resume wave re-serves
    /// anything the drain discarded, so nothing is lost.
    pub async fn suspend(&self) -> Result<(), TransportError> {
        let (reply, response) = oneshot::channel();
        self.cmds
            .send(Cmd::Suspend { reply })
            .map_err(|_| TransportError::Closed)?;
        response.await.map_err(|_| TransportError::Closed)
    }

    /// Come back onto the network after [`Self::suspend`]: re-opens with the
    /// same resume wave a wire flap uses (every leased topic from its kept
    /// position) and resolves at that wave's `CatchUpComplete` — "catch up,
    /// then done", the background-fetch primitive. With nothing leased, or a
    /// wire that never went down, it resolves immediately. An open failure
    /// does not fail this call: the transport keeps retrying on its backoff
    /// and the resolution rides to the resume wave that eventually completes.
    ///
    /// A process thawed *without* having suspended doesn't need this: the
    /// connection's own watchdog detects the stale wire and the transport
    /// reconnects transparently.
    pub async fn resume(&self) -> Result<(), TransportError> {
        let (reply, response) = oneshot::channel();
        self.cmds
            .send(Cmd::Resume { reply })
            .map_err(|_| TransportError::Closed)?;
        response.await.map_err(|_| TransportError::Closed)
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
    Suspend {
        reply: oneshot::Sender<()>,
    },
    Resume {
        reply: oneshot::Sender<()>,
    },
}

/// The wire opener: the initial `Mutate` (a first lease's adds, or a resume
/// wave) in, a fresh [`Connection`] out. Expressed as a trait with a blanket
/// impl — rather than `cfg`-gated `Box<dyn Fn .. + Send + Sync>` aliases — so
/// the split lives entirely in [`MaybeSend`]/[`MaybeSync`] (native-only
/// today, but the shape ports). `MaybeSync` because the ledger task holds a
/// shared reference across the open await (in [`reopen`]); it is only ever
/// *called* from that one task.
trait OpenWire<B: BidiBinding>: MaybeSend + MaybeSync {
    fn open(&self, initial: B::Mutate) -> BoxDynFuture<'static, Result<Connection<B>, OpenError>>;
}

impl<B: BidiBinding, F> OpenWire<B> for F
where
    F: Fn(B::Mutate) -> BoxDynFuture<'static, Result<Connection<B>, OpenError>>
        + MaybeSend
        + MaybeSync,
{
    fn open(&self, initial: B::Mutate) -> BoxDynFuture<'static, Result<Connection<B>, OpenError>> {
        self(initial)
    }
}

type Opener<B> = Box<dyn OpenWire<B>>;

/// What a resume wave restores: every leased topic with its resume cursor,
/// plus the leases whose `CatchUpComplete` the wave now owes.
type ResumeWave<C> = (Vec<(Topic, C)>, Vec<LeaseId>);

/// Whose `CatchUpComplete` a wave resolves.
enum WaveOwner {
    /// A lease's own add wave.
    Lease(LeaseId),
    /// A reconnect's resume wave: resolves every lease that was still
    /// catching up when the previous wire died (their own waves can never
    /// complete on the new wire), plus any [`BidiTransport::resume`] callers
    /// awaiting "caught up, then done".
    Resume {
        pending: Vec<LeaseId>,
        notify: Vec<oneshot::Sender<()>>,
    },
}

/// The topic ledger: which lease holds which topics, and where each in-flight
/// wave's `CatchUpComplete` should land.
struct Ledger<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    leases: HashMap<LeaseId, LeaseState<B>>,
    by_topic: HashMap<Topic, HashSet<LeaseId>>,
    /// The wire-position ledger: the furthest delivery ever seen per leased
    /// topic (advance-only) — the reconnect resume point. Not a durable
    /// cursor: it lives and dies with this transport.
    last_seen: HashMap<Topic, B::Cursor>,
    /// Whose `CatchUpComplete` each in-flight wave resolves.
    pending_waves: HashMap<WaveId, WaveOwner>,
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
    /// Frozen at lease time: each topic's add cursor. The catch-up-window
    /// filter — anything at-or-below it was never this lease's to receive
    /// (it belongs to a sibling's deeper replay).
    floors: HashMap<Topic, B::Cursor>,
    /// Advancing per-topic delivery positions; the post-catch-up filter.
    positions: HashMap<Topic, B::Cursor>,
    /// Set when this lease's add wave's `CatchUpComplete` arrives.
    caught_up: bool,
}

impl<B: TransportBinding> LeaseState<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    /// The delivery guarantee (module docs): during the catch-up window admit
    /// anything above the topic's frozen floor — filtering by the advancing
    /// position instead would let an early live delivery swallow the replay
    /// behind it — and once caught up admit only strictly past the position,
    /// so a caught-up lease sees each tracked message at most once.
    fn admit(&mut self, topic: &Topic, cursor: B::Cursor) -> bool {
        let Some(position) = self.positions.get_mut(topic) else {
            return true; // untracked topic — fail open
        };
        let admit = if self.caught_up {
            !B::covers(position, &cursor)
        } else {
            !self
                .floors
                .get(topic)
                .is_some_and(|floor| B::covers(floor, &cursor))
        };
        B::advance(position, cursor);
        admit
    }
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
            last_seen: HashMap::new(),
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

    fn register(
        &mut self,
        topics: Vec<Topic>,
        floors: HashMap<Topic, B::Cursor>,
        tx: mpsc::Sender<LeaseEvent<B>>,
    ) -> LeaseId {
        let id = LeaseId(self.next_lease);
        self.next_lease += 1;
        for topic in &topics {
            self.by_topic.entry(topic.clone()).or_default().insert(id);
        }
        self.leases.insert(
            id,
            LeaseState {
                tx,
                topics,
                positions: floors.clone(),
                floors,
                caught_up: false,
            },
        );
        id
    }

    /// Remove a lease; returns the topics that now have no interested lease
    /// (the ones to remove from the wire). Idempotent — a backpressure drop
    /// followed by the lease's own `Drop`-deref is harmless.
    fn deref(&mut self, id: LeaseId) -> Vec<Topic> {
        let Some(state) = self.leases.remove(&id) else {
            return Vec::new();
        };
        self.pending_waves.retain(|_, owner| match owner {
            WaveOwner::Lease(lease) => *lease != id,
            WaveOwner::Resume { pending, .. } => {
                pending.retain(|lease| *lease != id);
                true
            }
        });
        let mut removes = Vec::new();
        for topic in state.topics {
            if let Some(holders) = self.by_topic.get_mut(&topic) {
                holders.remove(&id);
                if holders.is_empty() {
                    self.by_topic.remove(&topic);
                    // Its wire position matters only while it can be resumed.
                    self.last_seen.remove(&topic);
                    removes.push(topic);
                }
            }
        }
        removes
    }

    /// Mark a lease caught up — its advancing position (now at the live edge)
    /// takes over from the floor as its delivery filter — and deliver its
    /// `CatchUpComplete`.
    fn complete(&mut self, lease: LeaseId, dropped: &mut Vec<LeaseId>) {
        if let Some(state) = self.leases.get_mut(&lease) {
            state.caught_up = true;
        }
        if !self.deliver(lease, LeaseEvent::CatchUpComplete) {
            dropped.push(lease);
        }
    }

    /// The adds for a resume wave that restores the whole wire: each leased
    /// topic from the deepest position still owed — `last_seen`, held down by
    /// the floor of any lease still catching up (its interrupted replay is
    /// re-owed in full) — plus the leases whose `CatchUpComplete` the wave
    /// now owes. `None` when nothing is leased.
    fn resume_adds(&self) -> Option<ResumeWave<B::Cursor>> {
        if self.by_topic.is_empty() {
            return None;
        }
        let mut adds = Vec::with_capacity(self.by_topic.len());
        for (topic, holders) in &self.by_topic {
            let mut resume = self.last_seen.get(topic).copied();
            let mut fallback = None;
            for lease in holders {
                let Some(state) = self.leases.get(lease) else {
                    continue;
                };
                let Some(floor) = state.floors.get(topic).copied() else {
                    continue;
                };
                fallback = Some(match fallback {
                    Some(other) => B::meet(other, floor),
                    None => floor,
                });
                if !state.caught_up {
                    resume = Some(match resume {
                        Some(seen) => B::meet(seen, floor),
                        None => floor,
                    });
                }
            }
            // All caught up with nothing delivered yet: the floors are the
            // only known positions (replaying below one is dedup'd anyway).
            let Some(cursor) = resume.or(fallback) else {
                continue;
            };
            adds.push((topic.clone(), cursor));
        }
        let pending = self
            .leases
            .iter()
            .filter(|(_, state)| !state.caught_up)
            .map(|(id, _)| *id)
            .collect();
        Some((adds, pending))
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
                match self.pending_waves.remove(&WaveId(mutate_id)) {
                    Some(WaveOwner::Lease(lease)) => self.complete(lease, &mut dropped),
                    Some(WaveOwner::Resume { pending, notify }) => {
                        for lease in pending {
                            self.complete(lease, &mut dropped);
                        }
                        // "Caught up, then done": the resume() callers resolve
                        // exactly here, at the resume wave's completion.
                        for waiter in notify {
                            let _ = waiter.send(());
                        }
                    }
                    None => {}
                }
            }
            Event::TopicsLive { topics } => {
                let mut per_lease: HashMap<LeaseId, Vec<Topic>> = HashMap::new();
                for topic in topics {
                    let Some(holders) = self.by_topic.get(&topic) else {
                        continue; // raced a deref — nobody cares anymore
                    };
                    for lease in holders {
                        // Only leases still catching up: a caught-up lease
                        // already heard this transition, and a resume wave's
                        // repeat would surface the flap we exist to hide.
                        if self.leases.get(lease).is_some_and(|state| state.caught_up) {
                            continue;
                        }
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
                let per_lease = self.demux(batch, B::group_topic, B::group_cursor, "group");
                for (lease, messages) in per_lease {
                    if !self.deliver(lease, LeaseEvent::GroupMessages(messages)) {
                        dropped.push(lease);
                    }
                }
            }
            Event::WelcomeMessages(batch) => {
                let per_lease = self.demux(batch, B::welcome_topic, B::welcome_cursor, "welcome");
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
    /// each lease's slice and applying each lease's delivery filter
    /// ([`LeaseState::admit`]) so the module's delivery guarantee holds.
    fn demux<M: Clone>(
        &mut self,
        batch: Vec<M>,
        topic_of: impl Fn(&M) -> Option<Topic>,
        cursor_of: impl Fn(&M) -> Option<B::Cursor>,
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
            let cursor = cursor_of(&message);
            if let Some(cursor) = cursor {
                // The wire-position ledger: the reconnect resume point.
                if let Some(seen) = self.last_seen.get_mut(&topic) {
                    B::advance(seen, cursor);
                } else {
                    self.last_seen.insert(topic.clone(), cursor);
                }
            }
            for lease in holders {
                let admit = match (cursor, self.leases.get_mut(lease)) {
                    (Some(cursor), Some(state)) => state.admit(&topic, cursor),
                    // No trackable cursor (or the lease is mid-drop this
                    // routing pass): fail open — the consumer's own dedup
                    // absorbs a duplicate, a silent drop loses data.
                    _ => true,
                };
                if admit {
                    per_lease.entry(*lease).or_default().push(message.clone());
                }
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
    /// The reconnect backoff elapsed on a dead wire with live leases.
    Reconnect,
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
    cmds: mpsc::UnboundedReceiver<Cmd<B>>,
    // For minting lease handles. Weak, so the task's own copy never keeps the
    // command channel (and therefore itself) alive.
    lease_cmds: mpsc::WeakUnboundedSender<Cmd<B>>,
) where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    LedgerTask {
        opener,
        cmds,
        lease_cmds,
        ledger: Ledger::default(),
        conn: None,
        reconnect_delay: RECONNECT_INITIAL_DELAY,
        reconnect_at: tokio::time::Instant::now(),
        suspended: false,
        resume_notify: Vec::new(),
        outbox: Outbox::default(),
        deferred: std::collections::VecDeque::new(),
    }
    .run()
    .await
}

/// How the ledger loop proceeds after handling a [`Step`].
enum Flow {
    Continue,
    /// Every handle and lease is gone — the task ends.
    Shutdown,
}

/// The ledger task's working state. Each [`Step`] the loop wakes up for is
/// handled by the same-named method; [`LedgerTask::run`] itself is only the
/// select-and-dispatch skeleton.
struct LedgerTask<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    opener: Opener<B>,
    cmds: mpsc::UnboundedReceiver<Cmd<B>>,
    /// For minting lease handles. Weak, so the task's own copy never keeps
    /// the command channel (and therefore itself) alive.
    lease_cmds: mpsc::WeakUnboundedSender<Cmd<B>>,
    ledger: Ledger<B>,
    conn: Option<Connection<B>>,
    reconnect_delay: std::time::Duration,
    /// When the next reconnect attempt is due. A fixed deadline, set when the
    /// wire dies and pushed out only by a failed attempt — recreating the
    /// sleep from `reconnect_delay` each loop iteration would let a steady
    /// stream of commands postpone the reconnect forever.
    reconnect_at: tokio::time::Instant,
    /// `suspend()`ed: the wire stays down on purpose — no lazy open for new
    /// leases, no reconnect backoff — until `resume()` clears it.
    suspended: bool,
    /// `resume()` callers awaiting a resume wave that hasn't been sent yet
    /// (the open failed, or hasn't happened). [`Self::reopen`] moves them
    /// onto the wave it sends; until then they park here across backoff
    /// retries.
    resume_notify: Vec<oneshot::Sender<()>>,
    /// Waves accepted but not yet on the wire, in issue order. Invariant:
    /// non-empty only while `conn` is `Some` — cleared on every wire close
    /// (stale waves are meaningless to a fresh wire; the resume wave
    /// re-seeds).
    outbox: Outbox<B::Mutate>,
    /// Commands absorbed while a dial was in flight (see
    /// [`Self::open_preemptibly`]), replayed in order before the channel is
    /// read again.
    deferred: std::collections::VecDeque<Cmd<B>>,
}

impl<B: TransportBinding> LedgerTask<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    async fn run(mut self) {
        loop {
            // Opportunistic flush: capacity frees when the actor makes
            // progress, and the actor's progress is what wakes this loop.
            self.flush_outbox();
            let flow = match self.next_step().await {
                // Loop back to the flush at the top.
                Step::Retry => Flow::Continue,
                Step::Cmd(None) => self.shutdown(),
                Step::Cmd(Some(Cmd::Lease { subs, depth, reply })) => {
                    self.lease(subs, depth, reply).await
                }
                Step::Cmd(Some(Cmd::Deref(id))) => self.deref(id),
                Step::Cmd(Some(Cmd::Suspend { reply })) => self.suspend(reply),
                Step::Cmd(Some(Cmd::Resume { reply })) => self.resume(reply).await,
                Step::Wire(Some(event)) => self.wire_event(event),
                Step::Wire(None) => self.wire_died(),
                Step::Reconnect => self.reconnect().await,
            };
            if let Flow::Shutdown = flow {
                return;
            }
        }
    }

    /// What this iteration wakes up for: a command deferred during a dial
    /// first, then whatever the wire state makes relevant.
    async fn next_step(&mut self) -> Step<B> {
        if let Some(cmd) = self.deferred.pop_front() {
            return Step::Cmd(Some(cmd));
        }
        match self.conn.as_mut() {
            Some(wire) => tokio::select! {
                cmd = self.cmds.recv() => Step::Cmd(cmd),
                event = wire.next() => Step::Wire(event),
                // Backstop for the rare quiet-wire case: the command buffer
                // was momentarily full and no event arrives to wake us.
                _ = tokio::time::sleep(OUTBOX_RETRY_INTERVAL), if !self.outbox.is_empty() => Step::Retry,
            },
            // A dead wire with live leases reconnects once the backoff
            // elapses, still absorbing commands meanwhile (which must not
            // move the deadline) — unless suspended, where staying off the
            // network is the whole point.
            None if !self.ledger.leases.is_empty() && !self.suspended => tokio::select! {
                cmd = self.cmds.recv() => Step::Cmd(cmd),
                _ = tokio::time::sleep_until(self.reconnect_at) => Step::Reconnect,
            },
            // No wire and either nothing leased or suspended: dormant until
            // the next command.
            None => Step::Cmd(self.cmds.recv().await),
        }
    }

    /// Every handle and lease is gone — close up shop.
    fn shutdown(&mut self) -> Flow {
        self.outbox.clear();
        if let Some(wire) = self.conn.take() {
            close_gracefully(wire);
        }
        Flow::Shutdown
    }

    /// `Cmd::Lease`: open the wire if this lease is the reason to have one,
    /// send a cursored (re-)add otherwise, and register the lease for
    /// routing.
    async fn lease(
        &mut self,
        subs: Vec<(Topic, B::Cursor)>,
        depth: usize,
        reply: oneshot::Sender<Result<TopicLease<B>, TransportError>>,
    ) -> Flow {
        let wave = self.ledger.next_wave_id();
        let topics: Vec<Topic> = subs.iter().map(|(topic, _)| topic.clone()).collect();
        let floors: HashMap<Topic, B::Cursor> = subs
            .iter()
            .map(|(topic, cursor)| (topic.clone(), *cursor))
            .collect();
        let mutate = B::build_mutate(subs, None, wave.0);
        let mut queued = None;
        if self.conn.is_none() && self.ledger.leases.is_empty() && !self.suspended {
            // Cold open, seeded with this lease's adds. The dial is
            // preemptible: commands keep flowing while it runs, and a
            // Suspend drops it. The failure is this caller's to see —
            // nobody else is waiting on the wire.
            match self.open_preemptibly(mutate).await {
                OpenOutcome::Opened(wire) => {
                    self.conn = Some(wire);
                    self.reconnect_delay = RECONNECT_INITIAL_DELAY;
                }
                OpenOutcome::Failed(e) => {
                    let _ = reply.send(Err(TransportError::Open(e)));
                    return Flow::Continue;
                }
                OpenOutcome::Suspended(ack) => {
                    // The dial is gone; the lease still registers below and
                    // rides the resume open, exactly like any lease taken
                    // while suspended.
                    self.suspend_preempted(ack);
                }
                OpenOutcome::Shutdown => return Flow::Shutdown,
            }
        } else if self.conn.is_some() {
            queued = Some(mutate);
        }
        // else: the wire is down (mid-backoff or suspended) — the next
        // resume wave (reconnect arm or `resume()`) covers this lease too,
        // so its mutate is deliberately dropped here.

        // Registered for routing immediately, NOT at CatchUpComplete: the
        // lease's own replay arrives *before* its CatchUpComplete and must
        // reach it. That in-flight deliveries for an already-live topic also
        // land right away is the documented non-monotonic delivery contract
        // (see module docs) — consumers identity-dedup until they are caught
        // up.
        let (tx, events) = mpsc::channel(depth.max(1));
        let id = self.ledger.register(topics.clone(), floors, tx);
        self.ledger.pending_waves.insert(wave, WaveOwner::Lease(id));
        if let Some(mutate) = queued {
            // Tagged with the lease so a deref can purge it if it never
            // reaches the wire.
            self.outbox.push(Some(id), mutate);
        }
        let Some(cmds) = self.lease_cmds.upgrade() else {
            // Command channel fully closed while we were opening; the loop
            // will see `None` next and shut down.
            return Flow::Continue;
        };
        let _ = reply.send(Ok(TopicLease {
            id,
            topics,
            events,
            cmds,
        }));
        Flow::Continue
    }

    /// `Cmd::Deref`: a lease handle dropped.
    fn deref(&mut self, id: LeaseId) -> Flow {
        // A wave the dropped lease never got onto the wire must not be sent
        // late: it would replay its topics from a cursor nobody asked for
        // anymore (siblings would receive unrequested backlog, and the
        // wave's completion would route to nobody).
        self.outbox.purge(id);
        let removes = self.ledger.deref(id);
        self.retire(removes);
        self.settle_idle_waiters();
        Flow::Continue
    }

    /// `Cmd::Suspend`: release the wire and stay off the network until
    /// `resume()`.
    fn suspend(&mut self, reply: oneshot::Sender<()>) -> Flow {
        tracing::info!(
            leases = self.ledger.leases.len(),
            had_wire = self.conn.is_some(),
            "bidi transport: suspending — going off the network"
        );
        self.suspended = true;
        self.outbox.clear();
        if let Some(wire) = self.conn.take() {
            // Detached on purpose: acknowledging must not await the wire's
            // bounded command channel (deadlock discipline above), and the
            // drain is bounded and discard-only — releasing the wire, not
            // finishing its funeral, is what "suspended" means.
            close_gracefully(wire);
        }
        // Pending waves stay: their leases are still owed a CatchUpComplete,
        // and `reopen` re-homes those obligations (and any parked resume()
        // waiters) onto the resume wave.
        let _ = reply.send(());
        Flow::Continue
    }

    /// `Cmd::Resume`: back onto the network; the reply resolves once the
    /// resume wave's catch-up completes ("catch up, then done").
    async fn resume(&mut self, reply: oneshot::Sender<()>) -> Flow {
        tracing::info!(
            leases = self.ledger.leases.len(),
            "bidi transport: resuming — catch up, then done"
        );
        self.suspended = false;
        if let Some(notify) = self
            .ledger
            .pending_waves
            .values_mut()
            .find_map(|owner| match owner {
                WaveOwner::Resume { notify, .. } => Some(notify),
                WaveOwner::Lease(_) => None,
            })
        {
            // A resume catch-up is already in flight — join it rather than
            // race it. If its wire has died meanwhile, hasten the reconnect
            // that will re-home this waiter.
            notify.push(reply);
            if self.conn.is_none() {
                self.reconnect_delay = RECONNECT_INITIAL_DELAY;
                self.reconnect_at = tokio::time::Instant::now();
            }
            Flow::Continue
        } else if self.conn.is_some() || self.ledger.leases.is_empty() {
            // A live wire (or nothing leased) has nothing to catch up.
            let _ = reply.send(());
            Flow::Continue
        } else {
            self.resume_notify.push(reply);
            self.reconnect_delay = RECONNECT_INITIAL_DELAY;
            self.reconnect().await
        }
    }

    /// A wire event: route it, and drop any lease that failed delivery.
    fn wire_event(&mut self, event: Event<B::GroupMessage, B::WelcomeMessage>) -> Flow {
        let mut removes = Vec::new();
        for lease in self.ledger.route(event) {
            removes.extend(self.ledger.deref(lease));
        }
        self.retire(removes);
        self.settle_idle_waiters();
        Flow::Continue
    }

    /// The wire ended (server close or transport failure). Leases survive:
    /// the dead-wire select arm reconnects with a resume wave (module docs)
    /// — no stream ever observes the flap.
    fn wire_died(&mut self) -> Flow {
        self.outbox.clear();
        drop(self.conn.take());
        self.reconnect_at = tokio::time::Instant::now() + self.reconnect_delay;
        if !self.ledger.leases.is_empty() {
            tracing::warn!("bidi transport: wire died; reconnecting from last-seen positions");
        }
        self.settle_idle_waiters();
        Flow::Continue
    }

    /// One reconnect attempt, absorbing every [`reopen`](Self::reopen)
    /// outcome: a dial-preempting suspend re-suspends, shutdown propagates.
    async fn reconnect(&mut self) -> Flow {
        match self.reopen().await {
            AfterReopen::Proceed => Flow::Continue,
            AfterReopen::Suspended(ack) => {
                self.suspend_preempted(ack);
                Flow::Continue
            }
            AfterReopen::Shutdown => Flow::Shutdown,
        }
    }

    /// A `Cmd::Suspend` preempted an in-flight dial: mark the transport
    /// suspended, outrank any resumes the dial had deferred, and acknowledge.
    fn suspend_preempted(&mut self, ack: oneshot::Sender<()>) {
        self.suspended = true;
        self.park_deferred_resumes();
        let _ = ack.send(());
    }

    /// Open a fresh wire seeded with the resume wave — every leased topic
    /// from its deepest owed position — and re-home every open obligation
    /// onto that wave: leases still owed a `CatchUpComplete`, `resume()`
    /// waiters parked in `resume_notify`, and waiters riding resume waves the
    /// previous wire never completed. On failure the backoff grows and every
    /// waiter stays parked for the next attempt. The dial is preemptible
    /// ([`Self::open_preemptibly`]): commands keep flowing while it runs, so
    /// a `suspend()` never waits on a stuck dial.
    async fn reopen(&mut self) -> AfterReopen {
        let Some((adds, pending)) = self.ledger.resume_adds() else {
            // Nothing leased: there is nothing to catch up on, so any
            // resume() waiters are already done.
            for waiter in self.resume_notify.drain(..) {
                let _ = waiter.send(());
            }
            return AfterReopen::Proceed;
        };
        let wave = self.ledger.next_wave_id();
        let mutate = B::build_mutate(adds, None, wave.0);
        match self.open_preemptibly(mutate).await {
            OpenOutcome::Opened(wire) => {
                self.conn = Some(wire);
                self.reconnect_delay = RECONNECT_INITIAL_DELAY;
                // Waves from the dead wire can never resolve on this one; the
                // resume wave owes every still-pending lease its
                // CatchUpComplete instead, and inherits the resume() waiters
                // those waves carried.
                let mut notify = std::mem::take(&mut self.resume_notify);
                for (_, owner) in self.ledger.pending_waves.drain() {
                    if let WaveOwner::Resume { notify: parked, .. } = owner {
                        notify.extend(parked);
                    }
                }
                self.ledger
                    .pending_waves
                    .insert(wave, WaveOwner::Resume { pending, notify });
                AfterReopen::Proceed
            }
            OpenOutcome::Failed(e) => {
                tracing::warn!("bidi transport: reconnect failed ({e}); backing off");
                self.reconnect_delay = (self.reconnect_delay * 2).min(RECONNECT_MAX_DELAY);
                self.reconnect_at = tokio::time::Instant::now() + self.reconnect_delay;
                // Resumes deferred mid-dial were concurrent with this attempt
                // and it failed for all of them: park their waiters on the
                // scheduled retry. Replaying them would grant each the
                // immediate-dial privilege reserved for a *fresh* resume().
                self.park_deferred_resumes();
                AfterReopen::Proceed
            }
            OpenOutcome::Suspended(ack) => AfterReopen::Suspended(ack),
            OpenOutcome::Shutdown => AfterReopen::Shutdown,
        }
    }

    /// Dial while still absorbing commands. Non-lifecycle commands arriving
    /// mid-dial are deferred in order (`next_step` drains `deferred` before
    /// reading the channel again); only `Suspend` — whose whole point is
    /// leaving the network *now* — and shutdown preempt the dial, dropping
    /// the in-flight open future.
    async fn open_preemptibly(&mut self, mutate: B::Mutate) -> OpenOutcome<B> {
        let open = self.opener.open(mutate);
        tokio::pin!(open);
        loop {
            tokio::select! {
                result = &mut open => {
                    return match result {
                        Ok(wire) => OpenOutcome::Opened(wire),
                        Err(e) => OpenOutcome::Failed(e),
                    };
                }
                cmd = self.cmds.recv() => match cmd {
                    Some(Cmd::Suspend { reply }) => return OpenOutcome::Suspended(reply),
                    Some(cmd) => self.deferred.push_back(cmd),
                    None => return OpenOutcome::Shutdown,
                },
            }
        }
    }

    /// A `Cmd::Resume` deferred during a dial must not replay as a fresh
    /// command. After a dial-preempting `Suspend`, replaying it would flip
    /// the transport back onto the network against the caller's newest
    /// intent; after a failed dial, replaying it would grant an immediate
    /// redial per deferred resume, stampeding the opener past its backoff.
    /// Park those waiters instead — they ride the next reopen's resume wave.
    /// (Everything else deferred — leases, derefs — replays safely.)
    fn park_deferred_resumes(&mut self) {
        for cmd in std::mem::take(&mut self.deferred) {
            match cmd {
                Cmd::Resume { reply } => self.resume_notify.push(reply),
                other => self.deferred.push_back(other),
            }
        }
    }

    /// With nothing leased there is no catch-up left to await: fire every
    /// parked `resume()` waiter — both those riding pending resume waves and
    /// those not yet on a wave — and drop the wave obligations, which can
    /// never resolve (their wire is gone or going, and nothing will re-open
    /// it). Called whenever a deref or wire death may have emptied the
    /// ledger; a no-op while any lease remains.
    fn settle_idle_waiters(&mut self) {
        if !self.ledger.leases.is_empty() {
            return;
        }
        for (_, owner) in self.ledger.pending_waves.drain() {
            if let WaveOwner::Resume { notify, .. } = owner {
                for waiter in notify {
                    let _ = waiter.send(());
                }
            }
        }
        for waiter in self.resume_notify.drain(..) {
            let _ = waiter.send(());
        }
    }

    /// Take unclaimed topics off the wire; close the wire when no lease is
    /// left. Never blocks — the removes wave rides the outbox (flushed at
    /// the top of the next loop iteration).
    fn retire(&mut self, removes: Vec<Topic>) {
        if self.ledger.leases.is_empty() {
            // Unsent waves are for a wire we're closing — nobody needs them.
            self.outbox.clear();
            if let Some(wire) = self.conn.take() {
                close_gracefully(wire);
            }
            return;
        }
        if removes.is_empty() || self.conn.is_none() {
            return;
        }
        self.outbox.push(None, B::build_mutate(None, removes, 0));
    }

    /// Push waves onto the wire until it's momentarily full, closed, or the
    /// outbox is empty. Never blocks; `Full` leaves the wave at the front
    /// for the next flush, `Closed` is left for the select's `Wire(None)` to
    /// clean up.
    fn flush_outbox(&mut self) {
        let Some(wire) = self.conn.as_ref() else {
            return;
        };
        while let Some((lease, wave)) = self.outbox.waves.pop_front() {
            match wire.try_mutate(wave) {
                Ok(()) => {}
                Err(TryMutateError::Full(wave)) | Err(TryMutateError::Closed(wave)) => {
                    self.outbox.waves.push_front((lease, wave));
                    return;
                }
            }
        }
    }
}

/// What racing a dial against the command channel produced. Non-lifecycle
/// commands arriving mid-dial are deferred in order (the ledger loop drains
/// `deferred` before reading the channel again); only `Suspend` — whose whole
/// point is leaving the network *now* — and shutdown preempt the dial,
/// dropping the in-flight open future.
enum OpenOutcome<B: BidiBinding> {
    Opened(Connection<B>),
    Failed(OpenError),
    /// A `Cmd::Suspend` arrived mid-dial and the dial was dropped. Carries
    /// the suspend reply for the caller to acknowledge once it has marked
    /// the transport suspended.
    Suspended(oneshot::Sender<()>),
    /// Every handle is gone; the ledger should shut down.
    Shutdown,
}

/// How the ledger loop should proceed after a [`LedgerTask::reopen`] attempt.
enum AfterReopen {
    Proceed,
    /// `Cmd::Suspend` preempted the dial: acknowledge once the caller has
    /// marked the transport suspended. Parked waiters stay parked and ride
    /// to the resume wave that eventually completes.
    Suspended(oneshot::Sender<()>),
    Shutdown,
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
        let drained = tokio::time::timeout(GRACEFUL_CLOSE_BUDGET, async {
            if wire.finish().await.is_err() {
                return; // actor already gone — nothing to drain
            }
            while wire.next().await.is_some() {}
        })
        .await;
        if drained.is_err() {
            // Expected-degraded, not an error: a long server-side catch-up
            // tail (or a process thaw after suspension) can outlive the
            // budget, and dropping the wire here is the designed ending.
            tracing::debug!(
                budget_ms = GRACEFUL_CLOSE_BUDGET.as_millis() as u64,
                "bidi transport: graceful-close drain budget expired; dropping the wire"
            );
        }
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

        /// Resolves at the next client `Ping`, panicking on anything else.
        async fn next_ping(&mut self) -> u64 {
            let frame = tokio::time::timeout(WAIT, self.from_client.recv())
                .await
                .expect("timed out waiting for a client frame")
                .expect("client closed the request stream");
            let Some(subscribe_request::Version::V1(v1)) = frame.version else {
                panic!("client sent unknown request version");
            };
            match v1.request.expect("client sent empty request") {
                subscribe_request::v1::Request::Ping(ping) => ping.nonce,
                other => panic!("expected a Ping, got {other:?}"),
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

    /// Once a lease is caught up, a sibling's deeper cursored re-add replays
    /// history on the shared wire — and the transport drops the copies: a
    /// caught-up lease receives each message at most once.
    #[xmtp_common::test(unwrap_try = true)]
    async fn caught_up_lease_never_sees_a_sibling_replay_twice() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;

        server.send(messages(
            vec![group_msg(1, b"g1"), group_msg(2, b"g1")],
            vec![],
        ));
        server.send(catchup_complete(first.mutate_id));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(1, b"g1"), group_msg(2, b"g1")])
            }
            _ => panic!("alpha expected its replay"),
        }
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // A sibling re-adds from zero: the server replays 1..2 plus new 3.
        let mut beta = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let second = server.next_mutate().await;
        server.send(messages(
            vec![
                group_msg(1, b"g1"),
                group_msg(2, b"g1"),
                group_msg(3, b"g1"),
            ],
            vec![],
        ));
        server.send(catchup_complete(second.mutate_id));

        // The window lease gets the full replay...
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(
                got,
                vec![
                    group_msg(1, b"g1"),
                    group_msg(2, b"g1"),
                    group_msg(3, b"g1"),
                ]
            ),
            _ => panic!("beta expected the full replay"),
        }
        // ...the caught-up lease only what it has not seen.
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(
                got,
                vec![group_msg(3, b"g1")],
                "replayed copies must be dropped for a caught-up lease"
            ),
            _ => panic!("alpha expected only the new message"),
        }
    }

    /// The window filter is the FROZEN floor, not the advancing position: an
    /// in-flight live delivery that leapfrogs the lease's own replay must not
    /// swallow the replay behind it.
    #[xmtp_common::test(unwrap_try = true)]
    async fn window_admits_the_replay_behind_an_early_live_delivery() {
        let (transport, servers) = transport();
        let mut lease = transport.lease(vec![(group_topic(b"g1"), 10)], 8).await?;
        let mut server = take_server(&servers);
        let wave = server.next_mutate().await;

        // Live 100 (in flight for the shared topic) lands before the replay.
        server.send(messages(vec![group_msg(100, b"g1")], vec![]));
        server.send(messages(vec![group_msg(11, b"g1")], vec![]));
        server.send(catchup_complete(wave.mutate_id));

        match recv(&mut lease).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(100, b"g1")])
            }
            _ => panic!("expected the early live delivery"),
        }
        match recv(&mut lease).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(
                got,
                vec![group_msg(11, b"g1")],
                "the replay behind an early live delivery must still deliver"
            ),
            _ => panic!("expected the replay"),
        }
        assert!(matches!(
            recv(&mut lease).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }

    /// During the window, deliveries at-or-below the lease's own add cursor
    /// are a sibling's deeper replay — never delivered to this lease.
    #[xmtp_common::test(unwrap_try = true)]
    async fn below_floor_history_is_not_delivered_during_the_window() {
        let (transport, servers) = transport();
        let mut high = transport.lease(vec![(group_topic(b"g1"), 10)], 8).await?;
        let mut server = take_server(&servers);
        server.next_mutate().await;

        // A sibling from zero triggers a deep replay on the shared wire.
        let mut deep = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        server.next_mutate().await;
        server.send(messages(
            vec![
                group_msg(5, b"g1"),
                group_msg(10, b"g1"),
                group_msg(11, b"g1"),
            ],
            vec![],
        ));

        match recv(&mut deep).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(
                got,
                vec![
                    group_msg(5, b"g1"),
                    group_msg(10, b"g1"),
                    group_msg(11, b"g1"),
                ]
            ),
            _ => panic!("the deep lease expected the full replay"),
        }
        match recv(&mut high).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(
                got,
                vec![group_msg(11, b"g1")],
                "history at-or-below the lease's own cursor is never its to receive"
            ),
            _ => panic!("the high lease expected only above-floor deliveries"),
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

    /// A fresh wire session parked by the reconnect (which fires on a timer,
    /// not synchronously with the death).
    async fn wait_for_server(servers: &Servers) -> MockServer {
        tokio::time::timeout(WAIT, async {
            loop {
                let next = {
                    let mut parked = servers.lock().unwrap();
                    (!parked.is_empty()).then(|| parked.remove(0))
                };
                if let Some(server) = next {
                    return server;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("timed out waiting for a reconnect open")
    }

    /// A wire flap is invisible to leases: the transport reconnects on its
    /// own and resumes each topic from its last-seen delivery position — and
    /// the caught-up lease receives only what it has not seen.
    #[xmtp_common::test(unwrap_try = true)]
    async fn wire_death_reconnects_from_last_seen_positions() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(messages(
            vec![group_msg(1, b"g1"), group_msg(2, b"g1")],
            vec![],
        ));
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        drop(server); // the wire dies mid-stream

        // The transport re-opens by itself; the resume wave re-adds the topic
        // from the last delivery the dead wire got to, not the lease's floor.
        let mut second = wait_for_server(&servers).await;
        let resume = second.next_mutate().await;
        assert_eq!(resume.adds.len(), 1);
        assert_eq!(resume.adds[0].topic, group_topic(b"g1").cloned_vec());
        assert_eq!(
            resume.adds[0].id_cursor, 2,
            "resume from last-seen, not the lease floor"
        );

        // The lease never ended: the replay overlap is filtered, new flows.
        second.send(messages(
            vec![group_msg(2, b"g1"), group_msg(3, b"g1")],
            vec![],
        ));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(3, b"g1")])
            }
            _ => panic!("the lease must survive the flap and see only new messages"),
        }
    }

    /// Command traffic on a dead wire must not move the reconnect deadline:
    /// the sleep is pinned when the wire dies, not recreated per absorbed
    /// command.
    #[xmtp_common::test(unwrap_try = true)]
    async fn command_traffic_does_not_postpone_the_reconnect() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        drop(server); // the wire dies

        // Churn lease/deref commands faster than the reconnect delay.
        let churn = {
            let transport = transport.clone();
            tokio::spawn(async move {
                loop {
                    let lease = transport.lease(vec![(group_topic(b"g9"), 0)], 4).await;
                    drop(lease);
                    tokio::time::sleep(Duration::from_millis(40)).await;
                }
            })
        };
        // The reconnect still fires on its original deadline.
        let _second = wait_for_server(&servers).await;
        churn.abort();
    }

    /// After a flap, the resume wave's TopicsLive must not reach a lease that
    /// was already caught up — it already heard that transition once.
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_caught_up_lease_hears_no_second_topics_live() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        let topic = group_topic(b"g1");
        server.send(topics_live(vec![&topic]));
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::TopicsLive(_))
        ));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        drop(server);
        let mut second = wait_for_server(&servers).await;
        let resume = second.next_mutate().await;
        second.send(topics_live(vec![&topic]));
        second.send(catchup_complete(resume.mutate_id));
        second.send(messages(vec![group_msg(1, b"g1")], vec![]));

        // The only thing alpha observes across the flap is new payload.
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(1, b"g1")])
            }
            _ => panic!("the flap leaked through (TopicsLive or a repeat CatchUpComplete)"),
        }
    }

    /// A *half-open* wire — silent but never closed — is caught by the
    /// connection actor's watchdog and lands in the same transparent-reconnect
    /// path as an outright close: one probe goes out on the dying wire, the
    /// actor tears down, the transport re-opens, and the lease never notices.
    #[xmtp_common::test(unwrap_try = true)]
    async fn half_open_wire_is_reaped_and_reconnected() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        // Advertise a fast keepalive so the watchdog runs at test speed, then
        // catch the lease up and go silent — without ever closing the wire.
        server.send(started(150));
        server.send(messages(vec![group_msg(1, b"g1")], vec![]));
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // The watchdog probes the silent wire first; nothing answers. Keep
        // `server` alive throughout — dropping it would close the wire and
        // exercise the ordinary flap path instead of the half-open one.
        server.next_ping().await;

        // The actor gives up and the transport re-opens on its own, resuming
        // from the last-seen position.
        let mut second = wait_for_server(&servers).await;
        let resume = second.next_mutate().await;
        assert_eq!(resume.adds.len(), 1);
        assert_eq!(resume.adds[0].id_cursor, 1);

        // The lease rides straight onto the new wire.
        second.send(messages(vec![group_msg(2, b"g1")], vec![]));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(2, b"g1")])
            }
            _ => panic!("the lease must survive a half-open wire invisibly"),
        }
    }

    /// `suspend()` takes the wire down as a graceful half-close and keeps the
    /// ledger; `resume()` re-opens from the kept positions and resolves at
    /// the resume wave's `CatchUpComplete` — catch up, then done.
    #[xmtp_common::test(unwrap_try = true)]
    async fn suspend_half_closes_and_resume_completes_at_catch_up() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(messages(vec![group_msg(1, b"g1")], vec![]));
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        transport.suspend().await?;
        // The wire goes down as a half-close, not an abort.
        server.request_stream_ended().await;

        let resumed = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });
        let mut second = wait_for_server(&servers).await;
        let resume = second.next_mutate().await;
        assert_eq!(resume.adds.len(), 1);
        assert_eq!(
            resume.adds[0].id_cursor, 1,
            "resume from the kept wire position"
        );
        second.send(catchup_complete(resume.mutate_id));
        tokio::time::timeout(WAIT, resumed).await?.unwrap()?;

        // The lease rode through the whole background/foreground cycle.
        second.send(messages(vec![group_msg(2, b"g1")], vec![]));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(2, b"g1")])
            }
            _ => panic!("the lease must survive suspend/resume invisibly"),
        }
    }

    /// Two concurrent `resume()` callers join the same in-flight catch-up:
    /// one wire, one resume wave, and both resolve at its `CatchUpComplete`.
    #[xmtp_common::test(unwrap_try = true)]
    async fn concurrent_resumes_join_one_catch_up_wave() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        transport.suspend().await?;
        server.request_stream_ended().await;

        let resume_a = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });
        let resume_b = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });

        let mut second = wait_for_server(&servers).await;
        let resume = second.next_mutate().await;
        second.send(catchup_complete(resume.mutate_id));
        tokio::time::timeout(WAIT, resume_a).await?.unwrap()?;
        tokio::time::timeout(WAIT, resume_b).await?.unwrap()?;
        assert!(
            servers.lock().unwrap().is_empty(),
            "the second resume must join the wave, not dial a second wire"
        );
    }

    /// While suspended the transport stays off the network — no reconnect
    /// backoff, no lazy open for new leases — and `resume()` covers both the
    /// kept lease and the one taken while suspended with one wave.
    #[xmtp_common::test(unwrap_try = true)]
    async fn suspended_transport_stays_off_the_network() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        transport.suspend().await?;
        server.request_stream_ended().await;

        // A lease taken while suspended registers but must not open a wire.
        let mut beta = transport.lease(vec![(group_topic(b"g2"), 0)], 8).await?;

        // Well past several reconnect backoffs: still nothing on the network.
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert!(
            servers.lock().unwrap().is_empty(),
            "suspended must mean off the network"
        );

        let resumed = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });
        let mut second = wait_for_server(&servers).await;
        let resume = second.next_mutate().await;
        let mut got: Vec<_> = resume.adds.iter().map(|add| add.topic.clone()).collect();
        got.sort();
        let mut want = vec![
            group_topic(b"g1").cloned_vec(),
            group_topic(b"g2").cloned_vec(),
        ];
        want.sort();
        assert_eq!(got, want, "one resume wave covers both leases");
        second.send(catchup_complete(resume.mutate_id));
        tokio::time::timeout(WAIT, resumed).await?.unwrap()?;

        // The suspended-time lease was owed its catch-up by the resume wave.
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }

    /// A `resume()` waiter whose last lease disappears mid-catch-up must
    /// resolve — with nothing leased there is nothing left to await.
    #[xmtp_common::test(unwrap_try = true)]
    async fn dropping_the_last_lease_settles_resume_waiters() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        transport.suspend().await?;
        server.request_stream_ended().await;

        // resume() re-opens and parks awaiting the resume wave's catch-up...
        let resumed = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });
        let mut second = wait_for_server(&servers).await;
        let _resume = second.next_mutate().await;
        // ...but the last lease goes away before the server answers.
        drop(alpha);
        tokio::time::timeout(WAIT, resumed).await?.unwrap()?;
    }

    /// `suspend()` must preempt a stuck dial: backgrounding during a network
    /// outage is exactly when dials hang, and leaving the network cannot
    /// wait on one.
    #[xmtp_common::test(unwrap_try = true)]
    async fn suspend_preempts_a_stuck_dial() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        // The first dial hangs forever; later dials get a scripted session.
        let servers: Servers = Arc::default();
        let dials = Arc::new(AtomicUsize::new(0));
        let transport: BidiTransport<V3Binding> = {
            let sink = servers.clone();
            let dials = dials.clone();
            BidiTransport::new(move |initial| {
                let n = dials.fetch_add(1, Ordering::SeqCst);
                let sink = sink.clone();
                async move {
                    if n == 0 {
                        std::future::pending::<()>().await;
                        unreachable!("the hung dial never resolves");
                    }
                    let (api, server) = mock_pair();
                    sink.lock().unwrap().push(server);
                    BidiConnection::open(&api, initial)
                        .await
                        .map_err(|e| Box::new(e) as OpenError)
                }
            })
        };

        // The cold open hangs; drive it from a task and wait for the dial
        // to actually be in flight.
        let leased = tokio::spawn({
            let transport = transport.clone();
            async move { transport.lease(vec![(group_topic(b"g1"), 0)], 8).await }
        });
        while dials.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        // Suspend preempts the stuck dial promptly...
        tokio::time::timeout(WAIT, transport.suspend()).await??;
        // ...and the lease registered anyway, parked for the resume open.
        let mut alpha = tokio::time::timeout(WAIT, leased).await?.unwrap()?;

        let resumed = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });
        let mut server = wait_for_server(&servers).await;
        let resume = server.next_mutate().await;
        server.send(catchup_complete(resume.mutate_id));
        tokio::time::timeout(WAIT, resumed).await?.unwrap()?;
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }

    /// A `resume()` deferred while a dial is in flight predates the
    /// `suspend()` that preempts the dial — replaying it afterwards must not
    /// put the transport back on the network. Its waiter parks and rides the
    /// next explicit resume instead.
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_preempting_suspend_outranks_a_deferred_resume() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let servers: Servers = Arc::default();
        let dials = Arc::new(AtomicUsize::new(0));
        let transport: BidiTransport<V3Binding> = {
            let sink = servers.clone();
            let dials = dials.clone();
            BidiTransport::new(move |initial| {
                let n = dials.fetch_add(1, Ordering::SeqCst);
                let sink = sink.clone();
                async move {
                    if n == 0 {
                        std::future::pending::<()>().await;
                        unreachable!("the hung dial never resolves");
                    }
                    let (api, server) = mock_pair();
                    sink.lock().unwrap().push(server);
                    BidiConnection::open(&api, initial)
                        .await
                        .map_err(|e| Box::new(e) as OpenError)
                }
            })
        };

        let leased = tokio::spawn({
            let transport = transport.clone();
            async move { transport.lease(vec![(group_topic(b"g1"), 0)], 8).await }
        });
        while dials.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        // FIFO on the command channel: this resume is deferred mid-dial,
        // then the suspend preempts the dial.
        let first_resume = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        tokio::time::timeout(WAIT, transport.suspend()).await??;
        let mut alpha = tokio::time::timeout(WAIT, leased).await?.unwrap()?;

        // The stale resume must not resurrect the network.
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert_eq!(
            dials.load(Ordering::SeqCst),
            1,
            "a resume deferred before the suspend must not redial"
        );

        // A later, explicit resume brings everything back — including the
        // parked waiter from before the suspend.
        let second_resume = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });
        let mut server = wait_for_server(&servers).await;
        let resume = server.next_mutate().await;
        server.send(catchup_complete(resume.mutate_id));
        tokio::time::timeout(WAIT, second_resume).await?.unwrap()?;
        tokio::time::timeout(WAIT, first_resume).await?.unwrap()?;
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }

    /// A burst of `resume()` calls during an outage collapses into ONE dial.
    /// The first resume earns the immediate attempt; resumes deferred while
    /// that dial is in flight rode it and lost with it — replaying each as a
    /// fresh dial would stampede the opener past its backoff. Their waiters
    /// park and ride the scheduled retry instead.
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_resume_burst_during_an_outage_dials_once() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let servers: Servers = Arc::default();
        let dials = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(tokio::sync::Notify::new());
        let network_down = Arc::new(AtomicBool::new(true));
        let transport: BidiTransport<V3Binding> = {
            let sink = servers.clone();
            let dials = dials.clone();
            let gate = gate.clone();
            let network_down = network_down.clone();
            BidiTransport::new(move |initial| {
                let n = dials.fetch_add(1, Ordering::SeqCst);
                let sink = sink.clone();
                let gate = gate.clone();
                let network_down = network_down.clone();
                async move {
                    // Dial 1 holds until released, then fails — the window
                    // for the rest of the burst to arrive mid-dial. Later
                    // dials fail fast until the network comes back.
                    if n == 1 {
                        gate.notified().await;
                        return Err(Box::new(std::io::Error::other("down")) as OpenError);
                    }
                    if n >= 2 && network_down.load(Ordering::SeqCst) {
                        return Err(Box::new(std::io::Error::other("down")) as OpenError);
                    }
                    let (api, server) = mock_pair();
                    sink.lock().unwrap().push(server);
                    BidiConnection::open(&api, initial)
                        .await
                        .map_err(|e| Box::new(e) as OpenError)
                }
            })
        };

        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        tokio::time::timeout(WAIT, transport.suspend()).await??;

        // The burst: the first resume dials immediately; two more land while
        // that dial is in flight and are deferred.
        let resumes: Vec<_> = (0..3)
            .map(|i| {
                let transport = transport.clone();
                tokio::spawn(async move {
                    if i > 0 {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                    transport.resume().await
                })
            })
            .collect();
        while dials.load(Ordering::SeqCst) < 2 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        network_down.store(false, Ordering::SeqCst);
        gate.notify_one();

        // Inside the backoff window (retry is scheduled at +200ms): the
        // deferred resumes must not have re-dialed on their own.
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            dials.load(Ordering::SeqCst),
            2,
            "resumes deferred by the failed dial must park, not re-dial"
        );

        // The scheduled retry carries every parked waiter to catch-up.
        let mut server = wait_for_server(&servers).await;
        let resume = server.next_mutate().await;
        server.send(catchup_complete(resume.mutate_id));
        for handle in resumes {
            tokio::time::timeout(WAIT, handle).await?.unwrap()?;
        }
        assert_eq!(dials.load(Ordering::SeqCst), 3, "exactly one retry dial");

        // And the lease is live on the new wire.
        server.send(messages(vec![group_msg(1, b"g1")], vec![]));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
    }

    /// `resume()` with nothing to catch up on — nothing leased, or a wire
    /// that never went down — resolves immediately and opens nothing.
    #[xmtp_common::test(unwrap_try = true)]
    async fn resume_with_nothing_to_do_resolves_immediately() {
        let (transport, servers) = transport();
        transport.suspend().await?;
        tokio::time::timeout(WAIT, transport.resume()).await??;

        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // On a live wire resume is a no-op that disturbs nothing.
        tokio::time::timeout(WAIT, transport.resume()).await??;
        server.send(messages(vec![group_msg(1, b"g1")], vec![]));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(
            servers.lock().unwrap().is_empty(),
            "resume on a live wire must not open a second one"
        );
    }

    /// A lease still catching up when the wire dies is re-owed its replay:
    /// the resume cursor is held down to its floor, and the resume wave's
    /// CatchUpComplete resolves its pending marker.
    #[xmtp_common::test(unwrap_try = true)]
    async fn interrupted_catch_up_is_re_owed_by_the_resume_wave() {
        let (transport, servers) = transport();
        let mut caught = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(messages(vec![group_msg(8, b"g1")], vec![]));
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut caught).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(matches!(
            recv(&mut caught).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // A second lease re-adds from 5 — and the wire dies before its replay.
        let mut pending = transport.lease(vec![(group_topic(b"g1"), 5)], 8).await?;
        server.next_mutate().await;
        drop(server);

        // The resume cursor is held DOWN to the pending lease's floor (5),
        // not last-seen (8): its interrupted replay is still owed.
        let mut second = wait_for_server(&servers).await;
        let resume = second.next_mutate().await;
        assert_eq!(resume.adds[0].id_cursor, 5);

        second.send(messages(
            vec![group_msg(6, b"g1"), group_msg(8, b"g1")],
            vec![],
        ));
        second.send(catchup_complete(resume.mutate_id));

        // The pending lease gets its replay AND its marker from the resume
        // wave...
        match recv(&mut pending).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(6, b"g1"), group_msg(8, b"g1")])
            }
            _ => panic!("pending lease expected its replay"),
        }
        assert!(matches!(
            recv(&mut pending).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        // ...while the caught-up lease skips the resume replay entirely.
        second.send(messages(vec![group_msg(9, b"g1")], vec![]));
        match recv(&mut caught).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(
                got,
                vec![group_msg(9, b"g1")],
                "the caught-up lease must skip the resume replay entirely"
            ),
            _ => panic!("caught-up lease expected only the new message"),
        }
    }

    /// A lease taken while the wire is down must not cold-open a wire of its
    /// own — the resume open covers the existing topics and the new lease's
    /// adds together, and every pending marker resolves from resume waves.
    #[xmtp_common::test(unwrap_try = true)]
    async fn lease_during_a_dead_wire_rides_the_resume_open() {
        let (transport, servers) = transport();
        let _alpha = transport.lease(vec![(group_topic(b"g1"), 3)], 8).await?;
        let server = take_server(&servers);
        drop(server);

        let mut beta = transport.lease(vec![(group_topic(b"g2"), 7)], 8).await?;
        let mut second = wait_for_server(&servers).await;

        // Depending on whether the lease raced the reconnect timer, its adds
        // ride the resume wave or a follow-up wave — but never a second wire.
        let mut adds: Vec<(Vec<u8>, u64)> = Vec::new();
        let mut waves = Vec::new();
        while adds.len() < 2 {
            let mutate = second.next_mutate().await;
            waves.push(mutate.mutate_id);
            adds.extend(
                mutate
                    .adds
                    .iter()
                    .map(|add| (add.topic.clone(), add.id_cursor)),
            );
        }
        adds.sort();
        let mut expected = vec![
            (group_topic(b"g1").cloned_vec(), 3),
            (group_topic(b"g2").cloned_vec(), 7),
        ];
        expected.sort();
        assert_eq!(adds, expected);
        assert!(
            servers.lock().unwrap().is_empty(),
            "one wire serves everyone"
        );

        for wave in waves {
            second.send(catchup_complete(wave));
        }
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }

    /// A lease dropped while the wire is dead needs no remove wave: deref
    /// prunes its topic from the ledger, so the resume wave simply never
    /// re-adds it — the fresh wire starts without the topic at all.
    #[xmtp_common::test(unwrap_try = true)]
    async fn deref_during_a_dead_wire_keeps_the_topic_off_the_resume_wave() {
        let (transport, servers) = transport();
        let _alpha = transport.lease(vec![(group_topic(b"g1"), 3)], 8).await?;
        let beta = transport.lease(vec![(group_topic(b"g2"), 7)], 8).await?;
        let server = take_server(&servers);
        drop(server); // the wire dies

        drop(beta); // deref lands while there is no wire to send a remove on

        let mut second = wait_for_server(&servers).await;
        let resume = second.next_mutate().await;
        assert_eq!(
            resume
                .adds
                .iter()
                .map(|add| (add.topic.clone(), add.id_cursor))
                .collect::<Vec<_>>(),
            vec![(group_topic(b"g1").cloned_vec(), 3)],
            "the dropped lease's topic must not ride the resume wave"
        );
        assert!(
            resume.removes.is_empty(),
            "nothing to remove on a fresh wire"
        );
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
