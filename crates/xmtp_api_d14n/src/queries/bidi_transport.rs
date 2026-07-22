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
//! - **The delivery guarantee: exactly-once per lease for cursor-tracked
//!   messages.** Every delivery frame carries the `mutate_id` of the wave
//!   that produced it (`0` = the live stream — XIP-83 delivery tags), the
//!   server serves live frames and each wave's replay in total cursor order
//!   per kind, and a topic owned by an in-flight wave receives NO live frames
//!   until that wave's `CatchUpComplete`. On those guarantees admission is
//!   structural — no per-lease positions, no consumer dedup contract:
//!   - a frame past `last_seen` (the wire's per-topic position) goes to
//!     **every holder** of its topic — the live fan-out, and for tagged
//!     frames the sibling-gap rule (below) — with one carve-out: a holder
//!     still owed history on the topic takes fresh frames only from a wave
//!     it rides (that IS its replay, streaming). From a foreign wave or the
//!     live lane the frame is **withheld** for it and flushed right before
//!     its `CatchUpComplete` — at most one event per kind, each in arrival
//!     order (per topic that is cursor order) and watermark-deduped —
//!     delivered now it would jump ahead of the owed replay, and a later
//!     overlapping replay would re-serve or skip it. Withheld frames are
//!     shared (`Arc`) across the holders withholding them and bounded per
//!     lease; a stalled catch-up that overflows the bound takes the
//!     channel-wedge recovery (the lease drops, its consumer re-leases
//!     from durable cursors);
//!   - a tagged frame at-or-below `last_seen` is **owed history**: it goes
//!     to each holder still catching up whose slice it falls in — above the
//!     holder's floor, at-or-below its registration snapshot (everything
//!     past the snapshot reaches it as it happens; with NO snapshot the
//!     wire had seen nothing of the topic, the owed edge is unknown, and
//!     everything above the floor is requested history until the catch-up
//!     closes the lane), and past what earlier replay passes already
//!     delivered it. The frame's own wave does not say whose history it
//!     carries: a lower re-add moves a topic into the re-adder's wave
//!     (XIP-83), so one lease's requested replay can ride a sibling's wave.
//!
//!   This client therefore **requires a tag-serving backend**
//!   (xmtp-node-go ≥ `6e0feb5f`, XIP-83 delivery tags). Against an older
//!   node that serves bidi without tags, replay frames arrive as tag `0`:
//!   a fresh wire can appear to work until a replay crosses the wire
//!   position and the live-order shield drops it — a reconnect or a
//!   shared-topic re-add makes that certain, and partial delivery would be
//!   the symptom. Such a backend is detected and refused instead of half
//!   served: its `CatchUpComplete` frames also carry no tag (`0`; ours are
//!   minted from 1), and the first one shuts the transport down — every
//!   lease ends and later `lease()`s see `Closed`, the tombstone the
//!   dispatch layer latches on to fall back to the legacy streams, which
//!   replay from durable cursors. There is deliberately no capability bit
//!   or untagged *fallback* at this layer (pre-GA the protocol just
//!   changes).
//!
//!   What this cannot cover: messages with no extractable cursor (fail open
//!   to every holder). Below a lease's own floor nothing is delivered on
//!   either lane — a shared topic's wire traffic can dip under a holder's
//!   floor (a sibling's lower re-add, a reconnect re-adding at the meet),
//!   and the per-lease floor guard skips those frames for that holder: its
//!   durable store already has them.
//! - **The sibling-gap rule.** A lease re-adding a shared topic at a lower
//!   cursor moves the topic into its wave, and live delivery for that topic
//!   stops — for *every* holder, not just the re-adder. The segment between
//!   the re-add and the wave's live crossover reaches the wire only inside
//!   the wave's replay, so the other holders' sole source for it is the
//!   sibling's tagged frames: the ledger forwards them exactly the
//!   `> last_seen` part, which is precisely what live delivery would have
//!   given them. This is also why `last_seen` is **per topic** rather than
//!   one high-water mark per wire: while one topic sits gapped inside a
//!   wave, its siblings' live frames (and cross-topic replay, on the shared
//!   per-kind cursor sequence) would push a global mark past the gap and
//!   mis-filter exactly the frames this rule exists to route.
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
//! returns — and rebuilds the wire's state as two kinds of wave:
//! - one **resume wave** re-adds every topic *not* owned by an interrupted
//!   wave, from `last_seen` — its replay is exactly the outage gap, so no
//!   holder sees a repeat;
//! - every lease still catching up — its wave interrupted mid catch-up, or
//!   its completion deferred on a sibling's wave — **re-issues** under a
//!   fresh wave id. Each topic re-adds at the meet of every stakeholder's
//!   position: each catching-up holder's outstanding owed-history edge (the
//!   wire position once nothing is owed below it), and a caught-up
//!   co-holder's `last_seen` (its floor when this wire never delivered) —
//!   issued once, on the draft with the lowest candidate, so the re-issues
//!   never yank each other apart. The owed-history lane dedups any overlap
//!   this hands a holder.
//!
//! Consumers never observe the flap. `next()` → `None` still means only: this
//! consumer fell behind and was dropped, or the transport itself shut down.
//!
//! ## Suspend / resume (the app-lifecycle touchpoint)
//!
//! [`BidiTransport::suspend`] half-closes the wire and parks: leases and wire
//! positions are kept, new leases register without touching the network, and
//! nothing reconnects until [`BidiTransport::resume`] — which re-opens with
//! the same reconnect scheme a wire flap uses and resolves once a live wire
//! has no wave left in flight: "catch up, then done", against everything
//! owed, the background-fetch primitive. (A wave issued meanwhile — a fresh
//! subscribe on the shared transport — extends the wait.)
//! An unpaired `resume()` (idle live wire, or nothing leased) resolves
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
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};
use xmtp_common::{BoxDynFuture, MaybeSend, MaybeSync, RetryableError};
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

/// Upper bound on frames withheld per lease, summed across its topics and
/// kinds ([`LeaseState::held_group`]/[`LeaseState::held_welcome`]). The
/// withhold window is one catch-up's lifetime — bounded on a healthy server
/// by its always-ack — so the cap only trips when a wave stalls while
/// traffic keeps flowing (a wedged or misbehaving server). Tripping
/// converts unbounded memory growth into the channel-wedge recovery: the
/// lease is dropped and its consumer re-leases from durable cursors.
const WITHHELD_FRAMES_CAP: usize = 512;
/// The most topics packed into one `Mutate` frame. A larger add-wave — a
/// lease over very many groups, or a reconnect resuming an agent's whole
/// subscription set — is split into this many topics per frame. A predictable
/// per-frame count (bounding memory and latency per wave) that works alongside
/// the byte budget below; whichever is reached first closes the frame. Chunks
/// of one lease share its wave owner, so the lease stays catching-up until its
/// last chunk resolves ([`LedgerTask::complete`]). Held as a field
/// ([`Ledger::chunk_cap`]) so tests shrink it per transport without a shared
/// global; production is always this value.
pub(crate) const MAX_MUTATE_TOPICS: usize = 1000;
/// Byte ceiling for one `Mutate` frame's topic payload — well under the
/// transport's 25 MiB encode limit, leaving headroom for the frame's own
/// framing. Bounding by *bytes*, not just count, keeps a frame under the limit
/// regardless of topic identifier length: [`MAX_MUTATE_TOPICS`] alone would not,
/// since a [`Topic`] is a `Vec<u8>` with no type-level size bound (today every
/// topic is a 17–33 B kind+id, but the guarantee should not rest on that).
pub(crate) const MAX_MUTATE_BYTES: usize = 16 * 1024 * 1024;
/// Upper bound on one entry's encoded overhead in a `Mutate` beyond its topic
/// bytes: the `id_cursor` varint (≤10 B) plus protobuf field tags and length
/// prefixes for the `Subscription`, generous enough to stay an upper bound
/// across both bindings' cursor shapes (a removes entry has no cursor, so this
/// over-counts it — safe, it only makes chunks smaller).
const PER_ENTRY_OVERHEAD: usize = 64;

/// Upper bound on the bytes one topic contributes to a `Mutate` frame: the
/// full topic bytes (kind byte + identifier) plus [`PER_ENTRY_OVERHEAD`]. An
/// over-estimate, so a byte-budgeted chunk never exceeds the budget.
fn topic_wire_cost(topic: &Topic) -> usize {
    1 + topic.identifier().len() + PER_ENTRY_OVERHEAD
}

/// Split `items` into chunks bounded by BOTH a count cap and an encoded-byte
/// budget, where `wire_cost` upper-bounds an item's encoded size. Each chunk
/// holds at least one item, so a single item larger than the budget still
/// makes progress (degenerate — no real topic approaches the budget) rather
/// than looping forever. Empty in → empty out.
fn chunk_by_budget<T>(
    items: Vec<T>,
    max_count: usize,
    max_bytes: usize,
    wire_cost: impl Fn(&T) -> usize,
) -> Vec<Vec<T>> {
    let mut chunks: Vec<Vec<T>> = Vec::new();
    let mut current: Vec<T> = Vec::new();
    let mut current_bytes = 0usize;
    for item in items {
        let cost = wire_cost(&item);
        if !current.is_empty() && (current.len() >= max_count || current_bytes + cost > max_bytes) {
            chunks.push(std::mem::take(&mut current));
            current_bytes = 0;
        }
        current_bytes += cost;
        current.push(item);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// Split a subscription add-set into per-frame chunks, each bounded to the
/// shared `Mutate` frame limits ([`MAX_MUTATE_TOPICS`] and [`MAX_MUTATE_BYTES`]).
/// The transport chunks its own waves internally; this is for callers that
/// build `history_only` `Mutate`s directly off the shared transport — the
/// bounded catch-up path — so their frames stay under the same encode ceiling
/// regardless of how many topics the client owns. Order-preserving; empty in →
/// empty out.
pub fn chunk_mutate_adds<C>(adds: Vec<(Topic, C)>) -> Vec<Vec<(Topic, C)>> {
    chunk_by_budget(adds, MAX_MUTATE_TOPICS, MAX_MUTATE_BYTES, |(topic, _)| {
        topic_wire_cost(topic)
    })
}

/// First retry after an unexpected wire death; doubles per failed attempt up
/// to [`RECONNECT_MAX_DELAY`]. Reconnection is never given up on — an offline
/// process resumes when the network returns.
const RECONNECT_INITIAL_DELAY: std::time::Duration = std::time::Duration::from_millis(100);
/// Ceiling for the exponential reconnect base. The armed delay adds up to a
/// full extra base of jitter ([`LedgerTask::arm_reconnect`]) to de-sync a
/// fleet, so the effective wait tops out near twice this.
const RECONNECT_MAX_DELAY: std::time::Duration = std::time::Duration::from_secs(30);
/// How long a wire must stay up before its death resets the backoff to the
/// floor. A wire that opens and dies inside this window is flapping, so its
/// backoff keeps growing instead — this stops an accept-then-immediately-close
/// server (an overloaded node RSTing right after accept) from pinning a client
/// at the reconnect floor. Only an open that *proves stable* earns the reset; a
/// bare accept no longer does.
const MIN_STABLE_UPTIME: std::time::Duration = std::time::Duration::from_secs(10);
/// How long a graceful close (half-close, then drain inbound to completion)
/// may take before the connection is dropped outright. After a half-close the
/// server finishes any in-flight catch-up waves before closing with `OK`
/// (immediately if none are in flight — XIP-83). Here that tail is data
/// nobody leases anymore, so this caps the discard-drain rather than waiting
/// out a deep replay.
const GRACEFUL_CLOSE_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

/// Error type an opener may return; boxed so the transport stays generic over
/// whichever API-client error the integration layer produces, while carrying
/// the inner error's retryability across the erasure: a transient dial
/// failure stays worth redialing, a backend that refuses the bidi surface
/// outright is not.
#[derive(Debug)]
pub struct OpenError {
    retryable: bool,
    source: Box<dyn std::error::Error + Send + Sync + 'static>,
}

impl OpenError {
    /// Capture `e` along with its own retryability verdict.
    pub fn new<E>(e: E) -> Self
    where
        E: std::error::Error + xmtp_common::RetryableError + Send + Sync + 'static,
    {
        Self {
            retryable: e.is_retryable(),
            source: Box::new(e),
        }
    }

    /// An open failure worth redialing (a transient dial or wire error).
    /// Test-only: production openers use [`Self::new`], whose verdict comes
    /// from the error itself and cannot contradict it.
    #[cfg(test)]
    pub fn retryable(e: impl Into<Box<dyn std::error::Error + Send + Sync + 'static>>) -> Self {
        Self {
            retryable: true,
            source: e.into(),
        }
    }

    /// An open failure no redial can fix (the backend refuses the surface).
    /// Test-only, like [`Self::retryable`].
    #[cfg(test)]
    pub fn unretryable(e: impl Into<Box<dyn std::error::Error + Send + Sync + 'static>>) -> Self {
        Self {
            retryable: false,
            source: e.into(),
        }
    }
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.source.fmt(f)
    }
}

impl std::error::Error for OpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.source)
    }
}

impl xmtp_common::RetryableError for OpenError {
    fn is_retryable(&self) -> bool {
        self.retryable
    }
}

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
    /// the per-topic `last_seen` and per-wave progress behind the module's
    /// delivery guarantee. `None` = untrackable; such a message fans out
    /// unfiltered and the consumer's own store absorbs it.
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
    /// Opening the wire failed; the lease was not registered. Whether a
    /// fresh `lease()` is worth attempting follows the inner error: a
    /// transient dial failure is, an outright backend refusal is not.
    #[error("opening the bidi wire failed: {0}")]
    Open(#[source] OpenError),
    /// A lease must name at least one topic: an adds-nothing wave yields no
    /// `CatchUpComplete` (XIP-83), so an empty lease could never resolve its
    /// markers — and it would pin the wire open while receiving nothing.
    #[error("a lease must name at least one topic")]
    Empty,
}

impl xmtp_common::RetryableError for TransportError {
    fn is_retryable(&self) -> bool {
        match self {
            // The opener knows whether a fresh `lease()` can succeed.
            Self::Open(e) => e.is_retryable(),
            // The transport is gone for good; an empty lease is a caller bug
            // no retry fixes.
            Self::Closed | Self::Empty => false,
        }
    }
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
    ///
    /// `initially_suspended` births the actor already [`Self::suspend`]ed:
    /// the first lease registers and parks instead of dialing, and nothing
    /// touches the network until [`Self::resume`]. For callers honoring a
    /// suspend intent recorded before the transport existed — an app
    /// launched into the background that suspends before anything streams.
    pub fn new<O, Fut>(opener: O, initially_suspended: bool) -> Self
    where
        O: Fn(B::Mutate) -> Fut + MaybeSend + MaybeSync + 'static,
        Fut: Future<Output = Result<Connection<B>, OpenError>> + MaybeSend + 'static,
    {
        Self::spawn(
            opener,
            initially_suspended,
            MAX_MUTATE_TOPICS,
            MAX_MUTATE_BYTES,
        )
    }

    /// [`Self::new`] with the per-`Mutate` chunk limits shrunk, so a test can
    /// cross the count or byte boundary without leasing thousands of topics or
    /// allocating megabytes. The limits are per-transport fields
    /// ([`Ledger::chunk_cap`]/[`Ledger::chunk_bytes`]), so concurrent tests
    /// never race on them; production always uses [`MAX_MUTATE_TOPICS`] /
    /// [`MAX_MUTATE_BYTES`].
    #[cfg(test)]
    pub(crate) fn new_with_chunk_limits<O, Fut>(
        opener: O,
        initially_suspended: bool,
        chunk_cap: usize,
        chunk_bytes: usize,
    ) -> Self
    where
        O: Fn(B::Mutate) -> Fut + MaybeSend + MaybeSync + 'static,
        Fut: Future<Output = Result<Connection<B>, OpenError>> + MaybeSend + 'static,
    {
        Self::spawn(opener, initially_suspended, chunk_cap, chunk_bytes)
    }

    fn spawn<O, Fut>(
        opener: O,
        initially_suspended: bool,
        chunk_cap: usize,
        chunk_bytes: usize,
    ) -> Self
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
            run_ledger::<B>(
                opener,
                cmds_rx,
                cmds.clone().downgrade(),
                initially_suspended,
                chunk_cap,
                chunk_bytes,
            ),
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
    /// any point is invisible here: the transport reconnects and re-issues
    /// this lease's wave (module docs), so `next()` → `None` means only
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
        self.enqueue_suspend()?
            .await
            .map_err(|_| TransportError::Closed)
    }

    /// Push a suspend command without awaiting its ack, returning the reply
    /// channel to await later. Splitting the enqueue (a synchronous push onto
    /// the unbounded command channel) from the await lets the process-level
    /// lifecycle fan-out push the command under the same lock that orders the
    /// suspend flag, so a concurrent resume cannot interleave its command send
    /// between that flag flip and this one.
    pub fn enqueue_suspend(&self) -> Result<oneshot::Receiver<()>, TransportError> {
        let (reply, response) = oneshot::channel();
        self.cmds
            .send(Cmd::Suspend { reply })
            .map_err(|_| TransportError::Closed)?;
        Ok(response)
    }

    /// Come back onto the network after [`Self::suspend`]: re-opens with the
    /// same reconnect scheme a wire flap uses (every leased topic from its
    /// kept position) and resolves once a live wire has no wave left in
    /// flight — "catch up, then done", against everything owed, the
    /// background-fetch primitive. A wave issued meanwhile (a fresh
    /// subscribe on the shared transport) extends the wait. With nothing
    /// leased, or an idle live wire, it resolves immediately. An open
    /// failure does not fail this call: the transport keeps retrying on its
    /// backoff and the resolution rides to the catch-up that eventually
    /// completes.
    ///
    /// A process thawed *without* having suspended doesn't need this: the
    /// connection's own watchdog detects the stale wire and the transport
    /// reconnects transparently.
    pub async fn resume(&self) -> Result<(), TransportError> {
        self.enqueue_resume()?
            .await
            .map_err(|_| TransportError::Closed)
    }

    /// Push a resume command without awaiting its catch-up, returning the reply
    /// channel to await later. The enqueue counterpart of [`Self::suspend`]'s
    /// [`Self::enqueue_suspend`] — see it for why the push is split from the
    /// await.
    pub fn enqueue_resume(&self) -> Result<oneshot::Receiver<()>, TransportError> {
        let (reply, response) = oneshot::channel();
        self.cmds
            .send(Cmd::Resume { reply })
            .map_err(|_| TransportError::Closed)?;
        Ok(response)
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

/// One chunked add-wave from [`Ledger::chunk_waves`]: its fresh wave id, the
/// topics it claims (for completion/blocker accounting), and the built
/// `Mutate` frame to send.
type ChunkWave<B> = (
    WaveId,
    HashMap<Topic, <B as TransportBinding>::Cursor>,
    <B as BidiBinding>::Mutate,
);

/// Whose `CatchUpComplete` a wave resolves.
enum WaveOwner {
    /// A lease's own add wave (or its reconnect re-issue). Its tagged frames
    /// are this lease's replay, routed to it unconditionally.
    Lease(LeaseId),
    /// A reconnect's resume wave. It has no owner lease for routing — its
    /// re-adds sit at `last_seen`, so its whole replay passes the sibling-gap
    /// filter and reaches every holder that way. `resume()` callers do not
    /// ride it: they park in the task's `resume_notify` and resolve when a
    /// live wire has no wave left in flight at all
    /// ([`LedgerTask::settle_caught_up_waiters`]).
    Resume,
}

/// Which of the wire's two delivery lanes a batch arrived on. v3 group and
/// welcome cursors are independent total-order sequences, so wave progress is
/// tracked per kind.
#[derive(Clone, Copy)]
enum DeliveryKind {
    Group,
    Welcome,
}

impl DeliveryKind {
    fn name(self) -> &'static str {
        match self {
            Self::Group => "group",
            Self::Welcome => "welcome",
        }
    }
}

/// An in-flight `Mutate` wave: whose `CatchUpComplete` it resolves, plus the
/// furthest tagged delivery per kind seen inside it. A wave's replay is
/// served in total cursor order per kind (XIP-83), so one cursor per kind
/// captures exactly what the wave has already delivered — the re-issue base
/// when a wire death interrupts its catch-up (see [`LedgerTask::reopen`]).
struct PendingWave<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    owner: WaveOwner,
    /// When the wave was issued — observability only: its `CatchUpComplete`
    /// logs the mutate→completion latency.
    issued_at: std::time::Instant,
    /// The wave's adds as sent: which topics it asked to replay, from where.
    /// A lower re-add claims a topic out of a sibling's wave server-side
    /// (XIP-83), so claims are what decide whether a resolving lease's
    /// history is still riding another wave (deferred completion, below).
    claims: HashMap<Topic, B::Cursor>,
    /// Completions deferred on this wave: leases whose own wave resolved
    /// while part of their topic history was still replaying here. Released
    /// (re-checked) when this wave resolves or is removed.
    holds: Vec<LeaseId>,
    progress_group: Option<B::Cursor>,
    progress_welcome: Option<B::Cursor>,
}

impl<B: TransportBinding> PendingWave<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    fn new(owner: WaveOwner, claims: HashMap<Topic, B::Cursor>) -> Self {
        Self {
            owner,
            issued_at: std::time::Instant::now(),
            claims,
            holds: Vec::new(),
            progress_group: None,
            progress_welcome: None,
        }
    }

    /// The wave's progress slot for `kind`'s cursor sequence.
    fn progress_mut(&mut self, kind: DeliveryKind) -> &mut Option<B::Cursor> {
        match kind {
            DeliveryKind::Group => &mut self.progress_group,
            DeliveryKind::Welcome => &mut self.progress_welcome,
        }
    }
}

/// The blueprint [`LedgerTask::reopen`] executes to rebuild a dead wire's
/// state (module docs): one resume wave plus a re-issue per interrupted
/// lease wave.
struct ReconnectPlan<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    /// The resume wave's adds: every topic no interrupted wave owns, at the
    /// wire's last-seen position (meet of its holders' floors when nothing
    /// was ever delivered). Empty when every leased topic rides a re-issue —
    /// then no resume wave is sent (an adds-nothing wave is refused).
    resume_adds: Vec<(Topic, B::Cursor)>,
    /// One re-issue per wave interrupted mid catch-up.
    reissues: Vec<Reissue<B>>,
    /// Leases still owed a `CatchUpComplete` whose re-issue came out empty:
    /// every topic they hold re-adds at a sibling's lower cursor, so the
    /// sibling's wave carries their replay. [`LedgerTask::reopen`] re-runs
    /// their completion once the new waves are registered — the deferred-
    /// completion check parks them on the wave that claimed their topic.
    orphans: Vec<LeaseId>,
}

/// One catching-up lease's re-issue: the same owner under a fresh wave id,
/// re-adding the lease's topics past everything already delivered to it.
struct Reissue<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    lease: LeaseId,
    adds: Vec<(Topic, B::Cursor)>,
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
    /// The wire's per-topic position: the furthest delivery routed per leased
    /// topic — live or tagged, advance-only (max-fold). It is both the
    /// reconnect re-add cursor and the sibling-gap filter ("what the live
    /// holders have already seen" — module docs). Not a durable cursor: it
    /// lives and dies with this transport.
    last_seen: HashMap<Topic, B::Cursor>,
    /// In-flight `Mutate` waves: whose `CatchUpComplete` each resolves, and
    /// how far its replay got (the re-issue base if the wire dies first).
    pending_waves: HashMap<WaveId, PendingWave<B>>,
    next_lease: u64,
    /// Wave correlation ids start at 1: `0` means "no correlation" on the wire.
    next_wave: u64,
    /// The most topics [`Ledger::chunk_waves`] packs into one `Mutate`, and the
    /// byte budget for the same frame. Production is [`MAX_MUTATE_TOPICS`] /
    /// [`MAX_MUTATE_BYTES`]; tests set them per transport so small inputs cross
    /// the chunk boundary — per-instance fields, not globals, so concurrent
    /// tests never race on them.
    chunk_cap: usize,
    chunk_bytes: usize,
}

struct LeaseState<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    tx: mpsc::Sender<LeaseEvent<B>>,
    topics: Vec<Topic>,
    /// Frozen at lease time: each topic's add cursor. Not a delivery filter —
    /// admission is structural (tag routing + the sibling-gap rule, module
    /// docs) — kept solely as the re-issue base when a wire death interrupts
    /// this lease's catch-up wave (see [`LedgerTask::reopen`]).
    floors: HashMap<Topic, B::Cursor>,
    /// The wire position (`last_seen`) per topic, frozen at registration.
    /// The upper edge of the lease's owed-history slice: everything past it
    /// the lease has received live or via the sibling-gap since it
    /// registered, so a replay frame at-or-below the snapshot is requested
    /// history (admit) and one between the snapshot and `last_seen` is a
    /// re-delivery (skip). Dropped once the lease is caught up — from there
    /// only fresh frames reach it.
    live_snap: HashMap<Topic, B::Cursor>,
    /// How far into the owed slice replays have delivered, per topic
    /// (max-fold on every owed-lane admission). The lower edge that keeps a
    /// second pass over the same history — a re-issue after a wire death, or
    /// a sibling's wave re-covering a yanked topic — from re-delivering it.
    /// Also the re-issue base: per topic, strictly tighter than a wave's
    /// per-kind progress. Dropped with the snapshot at catch-up.
    replayed: HashMap<Topic, B::Cursor>,
    /// Fresh-lane frames withheld while the lease is still owed history on
    /// their topic ([`LeaseState::owed_on`]) and the frame rides a FOREIGN
    /// wave or the live lane. Delivered immediately they would jump ahead
    /// of the owed replay, and — being invisible to the owed watermark —
    /// a later overlapping replay would re-serve or skip them. Flushed
    /// right before this lease's `CatchUpComplete` — at most one event per
    /// kind (never one per kind run, so the flush stays within the lease's
    /// channel bound), each in arrival order, watermark-deduped.
    /// They survive a reopen unscathed: the re-issue's replay either stays
    /// below them or re-delivers them through the owed lane, and the
    /// watermark skip at flush absorbs the overlap either way. One fresh
    /// frame may be withheld for several holders at once — the `Arc`
    /// shares the allocation between them (and with nothing else: delivery
    /// hands out owned clones). Each entry carries the lease's arrival
    /// sequence ([`LeaseState::withheld_seq`]) so the flush can merge each
    /// kind's topics back into arrival order. Bounded per lease by
    /// [`WITHHELD_FRAMES_CAP`]; dropped with the lease.
    held_group: Withheld<B::GroupMessage>,
    held_welcome: Withheld<B::WelcomeMessage>,
    /// Arrival stamp for the next withheld frame — per-topic vectors keep
    /// per-topic order on their own; this restores the order BETWEEN a
    /// kind's topics at flush time (and puts the first-arrived kind's
    /// event first).
    withheld_seq: u64,
    /// Waves already in flight when this lease registered. Their replays
    /// may have passed this lease's floor before it existed, so what
    /// remains of them is not gap-free for it — the owed lane never takes
    /// their frames for this lease (its own add wave, registered after,
    /// replays its full ask; skipping these loses nothing). Dead ids after
    /// a reopen are inert: waves die with their wire and re-issues mint
    /// fresh ids.
    preexisting: HashSet<WaveId>,
    /// Set when this lease's add wave's `CatchUpComplete` arrives.
    caught_up: bool,
}

/// A lease's withheld frames for one topic's kind, each stamped with the
/// lease's arrival sequence ([`LeaseState::withheld_seq`]) and sharing the
/// frame allocation with every other holder withholding it.
type Withheld<M> = HashMap<Topic, Vec<(u64, Arc<M>)>>;

impl<B: TransportBinding> LeaseState<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    /// Whether this lease is still owed replay history on `topic`. With a
    /// registration snapshot, owed means the floor sits below the snapshot
    /// and replays have not covered it yet. With NO snapshot the wire had
    /// seen nothing of the topic when the lease registered — which says
    /// nothing about what exists server-side — so the owed edge is unknown
    /// and the lease stays owed until its catch-up completes.
    fn owed_on(&self, topic: &Topic) -> bool {
        if self.caught_up {
            return false;
        }
        match self.live_snap.get(topic) {
            None => true,
            Some(snap) => {
                self.floors
                    .get(topic)
                    .is_none_or(|floor| !B::covers(floor, snap))
                    && !self
                        .replayed
                        .get(topic)
                        .is_some_and(|replayed| B::covers(replayed, snap))
            }
        }
    }

    /// Frames currently withheld for this lease, across topics and kinds —
    /// checked against [`WITHHELD_FRAMES_CAP`] before every buffer push.
    fn withheld(&self) -> usize {
        self.held_group.values().map(Vec::len).sum::<usize>()
            + self.held_welcome.values().map(Vec::len).sum::<usize>()
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
            chunk_cap: MAX_MUTATE_TOPICS,
            chunk_bytes: MAX_MUTATE_BYTES,
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

    /// Split an add-set into waves bounded by [`MAX_MUTATE_TOPICS`] and
    /// [`MAX_MUTATE_BYTES`] — one fresh wave id and one `Mutate` per chunk — so
    /// no single frame can exceed the encode limit however many topics a lease
    /// or a resume covers, and regardless of topic identifier length. Returns
    /// the per-chunk `(wave id, claims, mutate)`; the caller registers each as
    /// a [`PendingWave`] and sends or queues its mutate. Empty in → empty out
    /// (an adds-nothing wave is never sent).
    fn chunk_waves(&mut self, adds: Vec<(Topic, B::Cursor)>) -> Vec<ChunkWave<B>> {
        let groups = chunk_by_budget(adds, self.chunk_cap, self.chunk_bytes, |(topic, _)| {
            topic_wire_cost(topic)
        });
        let mut chunks = Vec::with_capacity(groups.len());
        for group in groups {
            let wave = self.next_wave_id();
            let claims: HashMap<Topic, B::Cursor> = group
                .iter()
                .map(|(topic, cursor)| (topic.clone(), *cursor))
                .collect();
            let mutate = B::build_mutate(group, None, wave.0);
            chunks.push((wave, claims, mutate));
        }
        chunks
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
        // Freeze the owed-history edge NOW: every frame past the current
        // wire position will reach this lease as it happens (live fan-out or
        // sibling-gap), so no replay may re-deliver that segment.
        let live_snap = topics
            .iter()
            .filter_map(|topic| {
                self.last_seen
                    .get(topic)
                    .map(|cursor| (topic.clone(), *cursor))
            })
            .collect();
        self.leases.insert(
            id,
            LeaseState {
                tx,
                topics,
                floors,
                live_snap,
                replayed: HashMap::new(),
                held_group: HashMap::new(),
                held_welcome: HashMap::new(),
                withheld_seq: 0,
                preexisting: self.pending_waves.keys().copied().collect(),
                caught_up: false,
            },
        );
        id
    }

    /// Remove a lease; returns the topics that now have no interested lease
    /// (the ones to remove from the wire). Completions deferred on this
    /// lease's waves are released (re-checked) — they may complete or re-park
    /// on another wave, and any of those deliveries can fail, so failures
    /// join the caller's `dropped` worklist. Idempotent — a backpressure drop
    /// followed by the lease's own `Drop`-deref is harmless.
    ///
    /// `purged` names the departing lease's waves that never reached the
    /// wire: only those are forgotten here. A wave already sent lives on the
    /// server regardless of its owner's departure — its replay is still
    /// arriving and may carry a surviving holder's owed history — so it
    /// stays pending, owner dangling, and resolves at its own
    /// `CatchUpComplete` (or a reopen's drain). Forgetting it here would
    /// re-check its held completions too early: the holder would go
    /// caught-up mid-replay and the owed-history lane would skip the very
    /// frames it was waiting for.
    fn deref(&mut self, id: LeaseId, purged: &[WaveId], dropped: &mut Vec<LeaseId>) -> Vec<Topic> {
        let Some(state) = self.leases.remove(&id) else {
            return Vec::new();
        };
        let mut released = Vec::new();
        self.pending_waves.retain(|wave_id, wave| match wave.owner {
            WaveOwner::Lease(lease) if lease == id && purged.contains(wave_id) => {
                released.append(&mut wave.holds);
                false
            }
            _ => true,
        });
        for wave in self.pending_waves.values_mut() {
            wave.holds.retain(|lease| *lease != id);
        }
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
        for lease in released {
            self.complete(lease, dropped);
        }
        removes
    }

    /// Mark a lease caught up — its wave is resolved, so from here its topics
    /// are fed live frames (plus whatever a sibling's wave holds back, via
    /// the sibling-gap rule) — and deliver its `CatchUpComplete`.
    ///
    /// Unless part of its history is still in flight elsewhere: a lower
    /// re-add moves a topic into the re-adder's wave (XIP-83), and a wave
    /// fully yanked that way resolves immediately — before the history its
    /// lease asked for has been delivered. Telling the lease "caught up" now
    /// would let a fetch-style consumer deref and lose that tail, so the
    /// completion parks on the blocking wave and is re-checked when that
    /// wave resolves (chains re-park hop by hop).
    ///
    /// Chains terminate: completions park on *waves*, never on each other
    /// (no cycle is expressible), a re-check happens only when a pending
    /// wave is consumed — resolved, dropped with its lease, or drained by a
    /// reopen — and each such release runs this method once per held lease,
    /// in a flat loop at the call sites. Depth costs one re-check per hop,
    /// never stack.
    fn complete(&mut self, lease: LeaseId, dropped: &mut Vec<LeaseId>) {
        let Some(state) = self.leases.get(&lease) else {
            return;
        };
        if state.caught_up {
            return;
        }
        // A wave still replaying below this lease's line for a topic — the
        // max of the topic's wire position and the lease's own floor; the
        // floor matters alone when nothing was ever delivered, and still
        // caps the line when the wire position trails the lease's durable
        // ask — holds history the lease has not received. A claim at-or-past
        // the line is forward-looking only.
        let last_seen = &self.last_seen;
        let blocker = self.pending_waves.values_mut().find(|wave| {
            // Another chunk of this lease's own add-set is still catching up.
            // A lease split across several waves (topic chunking) is not
            // caught up until its LAST wave resolves; the claims check below
            // only defers on a sibling's *lower* re-add, never on an own
            // same-floor chunk, so this arm carries the multi-wave-per-lease
            // case. A no-op when the lease has one wave: the completing wave
            // was already removed from `pending_waves` before this runs.
            matches!(wave.owner, WaveOwner::Lease(owner) if owner == lease)
                || state.topics.iter().any(|topic| {
                    wave.claims.get(topic).is_some_and(|claim| {
                        let below_wire = last_seen
                            .get(topic)
                            .is_some_and(|line| !B::covers(claim, line));
                        let below_floor = state
                            .floors
                            .get(topic)
                            .is_some_and(|line| !B::covers(claim, line));
                        below_wire || below_floor
                    })
                })
        });
        if let Some(wave) = blocker {
            if !wave.holds.contains(&lease) {
                wave.holds.push(lease);
            }
            return;
        }
        let mut groups: Vec<(u64, B::GroupMessage)> = Vec::new();
        let mut welcomes: Vec<(u64, B::WelcomeMessage)> = Vec::new();
        if let Some(state) = self.leases.get_mut(&lease) {
            state.caught_up = true;
            // Withheld fresh frames join the feed now, watermark-deduped
            // (a replay may have re-served them through the owed lane in
            // the meantime) — AHEAD of the completion marker: they are the
            // tail of history.
            for (topic, held) in std::mem::take(&mut state.held_group) {
                let replayed = state.replayed.get(&topic);
                groups.extend(
                    held.into_iter()
                        .filter(|(_, message)| {
                            B::group_cursor(message).is_some_and(|cursor| {
                                !replayed.is_some_and(|replayed| B::covers(replayed, &cursor))
                            })
                        })
                        .map(|(seq, message)| (seq, Arc::unwrap_or_clone(message))),
                );
            }
            for (topic, held) in std::mem::take(&mut state.held_welcome) {
                let replayed = state.replayed.get(&topic);
                welcomes.extend(
                    held.into_iter()
                        .filter(|(_, message)| {
                            B::welcome_cursor(message).is_some_and(|cursor| {
                                !replayed.is_some_and(|replayed| B::covers(replayed, &cursor))
                            })
                        })
                        .map(|(seq, message)| (seq, Arc::unwrap_or_clone(message))),
                );
            }
            // The owed-history lane closes with the catch-up; only fresh
            // frames follow.
            state.live_snap.clear();
            state.replayed.clear();
        }
        // The per-topic vectors preserve each topic's order on their own;
        // the arrival stamps restore the order BETWEEN topics, handing each
        // kind's event its slice of the wire in delivery order. At most one
        // event per kind — never one per kind run — so an alternating
        // group/welcome buffer flushes in at most three `try_send`s and
        // cannot burst a healthy lease past its channel bound. Cross-kind
        // interleaving is not observable live either: a wire batch is
        // single-kind.
        groups.sort_unstable_by_key(|(seq, _)| *seq);
        welcomes.sort_unstable_by_key(|(seq, _)| *seq);
        let welcomes_first = match (groups.first(), welcomes.first()) {
            (Some((group, _)), Some((welcome, _))) => welcome < group,
            _ => false,
        };
        let groups: Vec<_> = groups.into_iter().map(|(_, message)| message).collect();
        let welcomes: Vec<_> = welcomes.into_iter().map(|(_, message)| message).collect();
        let group_event = (!groups.is_empty()).then(|| LeaseEvent::GroupMessages(groups));
        let welcome_event = (!welcomes.is_empty()).then(|| LeaseEvent::WelcomeMessages(welcomes));
        let (first, second) = if welcomes_first {
            (welcome_event, group_event)
        } else {
            (group_event, welcome_event)
        };
        for event in [first, second].into_iter().flatten() {
            if !self.deliver(lease, event) {
                dropped.push(lease);
                return;
            }
        }
        if !self.deliver(lease, LeaseEvent::CatchUpComplete) {
            dropped.push(lease);
        }
    }

    /// The reconnect blueprint (module docs): a re-issue for every wave
    /// interrupted mid catch-up, and a resume wave covering the rest of the
    /// leased topics. `None` when nothing is leased. Read-only — the plan is
    /// applied by [`LedgerTask::reopen`] only once the dial succeeds, so a
    /// failed attempt leaves every obligation (and its progress) intact for
    /// the next one.
    fn reconnect_plan(&self) -> Option<ReconnectPlan<B>> {
        if self.by_topic.is_empty() {
            return None;
        }
        // Interrupted lease waves re-issue past what their owner has already
        // received; the topics they own are excluded from the resume wave
        // (the re-issue IS their replay, and a second, deeper re-add would
        // yank them straight back out of it). Ordered by original wave id,
        // then by lease for the wave-less drafts — the plan must not depend
        // on hash-map iteration.
        let mut interrupted: Vec<(WaveId, LeaseId)> = self
            .pending_waves
            .iter()
            .filter_map(|(id, wave)| match wave.owner {
                // A dangling owner (deref'd mid-replay) re-issues nothing:
                // its surviving holders re-issue themselves via the held
                // list below, and the reopen drain re-checks its holds.
                WaveOwner::Lease(lease) if self.leases.contains_key(&lease) => Some((*id, lease)),
                WaveOwner::Lease(_) => None,
                // A prior reconnect's resume wave: its tagged replay advanced
                // `last_seen`, so the new resume wave covers its topics.
                WaveOwner::Resume => None,
            })
            .collect();
        interrupted.sort_by_key(|(id, _)| id.0);
        let waved: HashSet<LeaseId> = interrupted.iter().map(|(_, lease)| *lease).collect();
        // A lease owed a deferred completion with no wave of its own (its
        // wave resolved fully-yanked; the tail was riding a sibling that
        // died with the wire) re-issues too: the owed-history lane dedups
        // whatever it already received.
        let mut held: Vec<LeaseId> = self
            .leases
            .iter()
            .filter(|(id, state)| !state.caught_up && !waved.contains(id))
            .map(|(id, _)| *id)
            .collect();
        held.sort_by_key(|id| id.0);

        // Draft one re-issue per catching-up lease, each topic at the
        // lease's own candidate — per topic, which is exactly right where a
        // wave's per-kind progress over-advances for a topic yanked out of
        // it: while owed history below the registration snapshot is still
        // outstanding, resume from its edge (the floor pushed past what
        // replays already delivered); once nothing is owed down there,
        // resume from the wire position itself.
        struct Draft<C> {
            lease: LeaseId,
            cands: Vec<(Topic, C)>,
        }
        let mut drafts: Vec<Draft<B::Cursor>> = Vec::new();
        let mut drafted: HashSet<LeaseId> = HashSet::new();
        for lease in interrupted.into_iter().map(|(_, lease)| lease).chain(held) {
            if !drafted.insert(lease) {
                continue;
            }
            let Some(state) = self.leases.get(&lease) else {
                continue;
            };
            if state.caught_up {
                continue; // nothing left to replay (defensive: completion removes the wave)
            }
            let mut cands = Vec::with_capacity(state.topics.len());
            for topic in &state.topics {
                // Floors cover every leased topic by construction.
                let Some(floor) = state.floors.get(topic).copied() else {
                    continue;
                };
                let owed = match state.live_snap.get(topic) {
                    // No registration snapshot: the owed edge is unknown
                    // (the wire had seen nothing of the topic), so resume
                    // from the floor pushed past what replays delivered —
                    // never from the wire position, which may sit past
                    // history this lease was never served.
                    None => true,
                    Some(snap) => {
                        !B::covers(&floor, snap)
                            && state
                                .replayed
                                .get(topic)
                                .is_none_or(|replayed| !B::covers(replayed, snap))
                    }
                };
                let cursor = if owed {
                    let mut cursor = floor;
                    if let Some(replayed) = state.replayed.get(topic) {
                        B::advance(&mut cursor, *replayed);
                    }
                    cursor
                } else {
                    self.last_seen.get(topic).copied().unwrap_or(floor)
                };
                cands.push((topic.clone(), cursor));
            }
            drafts.push(Draft { lease, cands });
        }

        // Per topic: the meet of every candidate is the safe re-add cursor,
        // and the draft with the lowest candidate is the wave that carries
        // it (a lower re-add claims the topic server-side anyway — issuing
        // it once avoids yanking our own re-issues apart).
        struct TopicPlan<C> {
            meet: C,
            best: usize,
        }
        let mut plans: HashMap<&Topic, TopicPlan<B::Cursor>> = HashMap::new();
        for (idx, draft) in drafts.iter().enumerate() {
            for (topic, cursor) in &draft.cands {
                plans
                    .entry(topic)
                    .and_modify(|plan| {
                        let lower = !B::covers(cursor, &plan.meet);
                        plan.meet = B::meet(plan.meet, *cursor);
                        if lower {
                            plan.best = idx;
                        }
                    })
                    .or_insert(TopicPlan {
                        meet: *cursor,
                        best: idx,
                    });
            }
        }
        // A caught-up holder's feed resumes exactly at the wire position —
        // or, when nothing was ever delivered on this wire, at its own
        // floor: fold it into the meet so the re-add also replays its
        // outage gap (delivered to it as fresh frames; below the
        // re-issuing owner's floor it is the documented carve-out).
        for (topic, plan) in plans.iter_mut() {
            let Some(holders) = self.by_topic.get(*topic) else {
                continue;
            };
            for lease in holders {
                let Some(state) = self.leases.get(lease) else {
                    continue;
                };
                if !state.caught_up {
                    continue;
                }
                // Caught up = has received everything up to the wire
                // position, so that is the holder's resume line. With no
                // wire position at all (nothing ever delivered for the
                // topic on this wire), its catch-up covered exactly up to
                // its own floor — fold that instead. Skipping the holder
                // would let the meet land above its gap.
                let Some(line) = self
                    .last_seen
                    .get(*topic)
                    .or_else(|| state.floors.get(*topic))
                else {
                    continue;
                };
                plan.meet = B::meet(plan.meet, *line);
            }
        }
        let mut reissues = Vec::with_capacity(drafts.len());
        let mut orphans = Vec::new();
        for (idx, draft) in drafts.iter().enumerate() {
            let adds: Vec<(Topic, B::Cursor)> = draft
                .cands
                .iter()
                .filter_map(|(topic, _)| {
                    let plan = plans.get(topic)?;
                    (plan.best == idx).then(|| (topic.clone(), plan.meet))
                })
                .collect();
            if adds.is_empty() {
                orphans.push(draft.lease);
            } else {
                reissues.push(Reissue {
                    lease: draft.lease,
                    adds,
                });
            }
        }
        let owned: HashSet<Topic> = plans.keys().map(|topic| (*topic).clone()).collect();
        let mut resume_adds = Vec::with_capacity(self.by_topic.len());
        for (topic, holders) in &self.by_topic {
            if owned.contains(topic) {
                continue;
            }
            let cursor = self.last_seen.get(topic).copied().or_else(|| {
                // Never delivered anything: the holders' floors are the only
                // known positions, and their meet is the safe re-add —
                // replaying below a holder's floor is history it never asked
                // for, which its durable store already covers (the one seam
                // exactly-once does not; module docs).
                let mut meet = None;
                for lease in holders {
                    let Some(floor) = self
                        .leases
                        .get(lease)
                        .and_then(|state| state.floors.get(topic))
                        .copied()
                    else {
                        continue;
                    };
                    meet = Some(match meet {
                        Some(other) => B::meet(other, floor),
                        None => floor,
                    });
                }
                meet
            });
            let Some(cursor) = cursor else {
                continue;
            };
            resume_adds.push((topic.clone(), cursor));
        }
        Some(ReconnectPlan {
            resume_adds,
            reissues,
            orphans,
        })
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
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::debug!(
                    lease = id.0,
                    "bidi transport: lease receiver dropped; releasing it"
                );
                false
            }
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
                if let Some(wave) = self.pending_waves.remove(&WaveId(mutate_id)) {
                    tracing::debug!(
                        wave = mutate_id,
                        elapsed_ms = wave.issued_at.elapsed().as_millis() as u64,
                        topics = wave.claims.len(),
                        deferred_completions = wave.holds.len(),
                        waves_left = self.pending_waves.len(),
                        "bidi transport: catch-up wave resolved"
                    );
                    if let WaveOwner::Lease(lease) = wave.owner {
                        self.complete(lease, &mut dropped);
                    }
                    // Completions parked on this wave re-check: they finish
                    // here, or re-park on the next wave still replaying part
                    // of their history (yank chains resolve hop by hop).
                    for lease in wave.holds {
                        self.complete(lease, &mut dropped);
                    }
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
            Event::GroupMessages {
                messages,
                mutate_id,
            } => {
                let per_lease = self.demux(
                    messages,
                    mutate_id,
                    B::group_topic,
                    B::group_cursor,
                    |state| &mut state.held_group,
                    DeliveryKind::Group,
                    &mut dropped,
                );
                for (lease, messages) in per_lease {
                    if !self.deliver(lease, LeaseEvent::GroupMessages(messages)) {
                        dropped.push(lease);
                    }
                }
            }
            Event::WelcomeMessages {
                messages,
                mutate_id,
            } => {
                let per_lease = self.demux(
                    messages,
                    mutate_id,
                    B::welcome_topic,
                    B::welcome_cursor,
                    |state| &mut state.held_welcome,
                    DeliveryKind::Welcome,
                    &mut dropped,
                );
                for (lease, messages) in per_lease {
                    if !self.deliver(lease, LeaseEvent::WelcomeMessages(messages)) {
                        dropped.push(lease);
                    }
                }
            }
        }
        dropped
    }

    /// Group a delivery batch by interested lease, preserving wire order
    /// within each lease's slice and applying the tag-routing rules behind
    /// the module's exactly-once guarantee: a frame past the wire position
    /// fans out to every holder of its topic — withheld for holders still
    /// owed history there, unless the frame rides their own wave; a tagged
    /// frame at-or-below it goes to each catching-up holder's owed-history
    /// slice (module docs).
    // Private, with per-kind closure plumbing — a param struct would cost
    // more than the count saves.
    #[allow(clippy::too_many_arguments)]
    fn demux<M: Clone>(
        &mut self,
        batch: Vec<M>,
        mutate_id: u64,
        topic_of: impl Fn(&M) -> Option<Topic>,
        cursor_of: impl Fn(&M) -> Option<B::Cursor>,
        held_of: impl Fn(&mut LeaseState<B>) -> &mut Withheld<M>,
        kind: DeliveryKind,
        dropped: &mut Vec<LeaseId>,
    ) -> HashMap<LeaseId, Vec<M>> {
        let wave = (mutate_id != 0).then_some(WaveId(mutate_id));
        // Who rides this wave — its owner and its deferred completions. A
        // rider's tagged frames are its own replay streaming in: they are
        // never withheld, whatever the fresh/owed classification says.
        let riders: (Option<LeaseId>, Vec<LeaseId>) = wave
            .and_then(|wave| self.pending_waves.get(&wave))
            .map(|pending| {
                let owner = match pending.owner {
                    WaveOwner::Lease(lease) => Some(lease),
                    WaveOwner::Resume => None,
                };
                (owner, pending.holds.clone())
            })
            .unwrap_or((None, Vec::new()));
        let mut per_lease: HashMap<LeaseId, Vec<M>> = HashMap::new();
        for message in batch {
            // One shared allocation per frame: withheld copies are Arc
            // clones; deliveries hand out owned clones of the same frame.
            let message = Arc::new(message);
            let Some(topic) = topic_of(&message) else {
                // A delivery we cannot key by topic cannot be routed. Skip just
                // this message — the rest of the batch is unaffected.
                tracing::warn!(
                    "bidi transport: dropping unroutable {} message",
                    kind.name()
                );
                continue;
            };
            let Some(holders) = self.by_topic.get(&topic) else {
                // Deliveries can trail a deref; nobody leases this topic now.
                tracing::debug!(%topic, "bidi transport: delivery for an unleased topic");
                continue;
            };
            let Some(cursor) = cursor_of(&message) else {
                // No trackable cursor: fail open to every holder — the
                // consumer's own store absorbs a duplicate, a silent drop
                // loses data — and advance nothing.
                for lease in holders {
                    if dropped.contains(lease) {
                        continue;
                    }
                    per_lease
                        .entry(*lease)
                        .or_default()
                        .push((*message).clone());
                }
                continue;
            };
            // The sibling-gap filter: "what the live holders have not seen".
            let fresh = !self
                .last_seen
                .get(&topic)
                .is_some_and(|seen| B::covers(seen, &cursor));
            // Where this frame's wave started replaying the topic — the
            // owed lane's gap guard (below). `None` for the live lane, or
            // for a wave this ledger no longer tracks.
            let claim: Option<B::Cursor> = wave
                .and_then(|wave| self.pending_waves.get(&wave))
                .and_then(|pending| pending.claims.get(&topic))
                .copied();
            if wave.is_none() && !fresh {
                // Live frames are total-cursor-ordered and never overlap a
                // wave's replay (server guarantees); a covered live frame is
                // a server fault — shield the exactly-once guarantee.
                tracing::warn!(%topic, "bidi transport: dropping an out-of-order live delivery");
                continue;
            }
            for lease in holders {
                if dropped.contains(lease) {
                    continue; // overflowed this batch; being torn down
                }
                if fresh {
                    // Past the wire position, every holder is owed the frame,
                    // whichever lane carries it (live fan-out / sibling-gap) —
                    // except below the holder's own floor: that is history it
                    // explicitly resumed past (its durable store already has
                    // it), reachable here when a floor runs ahead of the wire
                    // position. Mirrors the owed lane's floor guard.
                    let Some(state) = self.leases.get_mut(lease) else {
                        continue;
                    };
                    if state
                        .floors
                        .get(&topic)
                        .is_some_and(|floor| B::covers(floor, &cursor))
                    {
                        continue;
                    }
                    // A holder still owed history on this topic cannot take
                    // fresh frames directly — delivered now they would jump
                    // ahead of the owed replay. With a snapshot the owed
                    // slice is BOUNDED and everything fresh sits above it:
                    // withhold it all (its own wave's crossover included)
                    // until the slice completes. With NO snapshot the owed
                    // range is open-ended, and the frames of a wave this
                    // lease rides ARE its replay streaming in: deliver
                    // those — draining any withheld frames below them
                    // first, in order — and advance the watermark so an
                    // overlapping foreign replay dedups against them;
                    // foreign frames are withheld.
                    let rides = riders.0 == Some(*lease) || riders.1.contains(lease);
                    if state.owed_on(&topic) {
                        if !rides || state.live_snap.contains_key(&topic) {
                            // Bounded: a stalled catch-up must not buffer
                            // forever. Overflow takes the channel-wedge
                            // recovery path — drop the lease, free its
                            // buffers; its consumer re-leases from durable
                            // cursors.
                            if state.withheld() >= WITHHELD_FRAMES_CAP {
                                state.held_group.clear();
                                state.held_welcome.clear();
                                tracing::warn!(
                                    %topic,
                                    "bidi transport: withheld-frame cap hit mid catch-up; \
                                     dropping the lease (re-lease from durable cursors to recover)"
                                );
                                dropped.push(*lease);
                                continue;
                            }
                            let seq = state.withheld_seq;
                            state.withheld_seq += 1;
                            held_of(state)
                                .entry(topic.clone())
                                .or_default()
                                .push((seq, message.clone()));
                            continue;
                        }
                        let out = per_lease.entry(*lease).or_default();
                        let watermark = state.replayed.get(&topic).copied();
                        if let Some(held) = held_of(state).get_mut(&topic) {
                            let mut kept = Vec::new();
                            for (seq, withheld) in held.drain(..) {
                                match cursor_of(&withheld) {
                                    Some(c) if B::covers(&cursor, &c) => {
                                        // Strictly below this frame and past
                                        // the watermark: joins ahead of it.
                                        // An equal or already-covered copy
                                        // is a duplicate — drop it.
                                        let below = !B::covers(&c, &cursor);
                                        let covered = watermark
                                            .is_some_and(|replayed| B::covers(&replayed, &c));
                                        if below && !covered {
                                            out.push(Arc::unwrap_or_clone(withheld));
                                        }
                                    }
                                    _ => kept.push((seq, withheld)),
                                }
                            }
                            *held = kept;
                        }
                        match state.replayed.get_mut(&topic) {
                            Some(replayed) => B::advance(replayed, cursor),
                            None => {
                                state.replayed.insert(topic.clone(), cursor);
                            }
                        }
                        out.push((*message).clone());
                        continue;
                    }
                    per_lease
                        .entry(*lease)
                        .or_default()
                        .push((*message).clone());
                    continue;
                }
                // The owed-history lane. A lease's requested replay may ride
                // ANY wave — a lower re-add moves the topic into the
                // re-adder's wave (XIP-83), so the frame's tag cannot say
                // whose history it carries. Each holder still catching up
                // admits exactly the slice it asked for and has not
                // received: above its floor, at-or-below its registration
                // snapshot (everything past that reached it as it happens —
                // with NO snapshot the owed edge is unknown, so everything
                // above the floor is requested history), and past what
                // earlier replay passes already delivered.
                let Some(state) = self.leases.get_mut(lease) else {
                    continue;
                };
                let within_snap = match state.live_snap.get(&topic) {
                    None => true,
                    Some(snap) => B::covers(snap, &cursor),
                };
                if state.caught_up
                    || !within_snap
                    || state
                        .floors
                        .get(&topic)
                        .is_some_and(|floor| B::covers(floor, &cursor))
                {
                    continue;
                }
                // Gap guards: the watermark is a high-water mark, so this
                // holder may only take owed frames from a wave whose replay
                // is gap-free below the holder's progress. Two ways it can
                // not be: the wave asked from ABOVE max(holder floor,
                // watermark) — its replay never covers what the holder
                // still needs underneath — or the wave predates the holder
                // and may have replayed past its floor before it existed.
                // Either way the frames would jump the watermark over
                // history still owed below (lost for good once "covered");
                // skipped here, they re-arrive inside the holder's own
                // wave, in order, and dedup at the watermark.
                let reachable = claim.is_some_and(|claim| {
                    let mut edge = state.floors.get(&topic).copied().unwrap_or(claim);
                    if let Some(replayed) = state.replayed.get(&topic) {
                        B::advance(&mut edge, *replayed);
                    }
                    B::covers(&edge, &claim)
                });
                if !reachable || wave.is_some_and(|wave| state.preexisting.contains(&wave)) {
                    continue;
                }
                let watermark = state.replayed.get(&topic).copied();
                if watermark.is_some_and(|replayed| B::covers(&replayed, &cursor)) {
                    continue;
                }
                let out = per_lease.entry(*lease).or_default();
                // Withheld fresh frames below this one join the feed ahead
                // of it, in order (only the open owed range can have any —
                // with a snapshot everything withheld sits above the slice,
                // so above this frame too). A withheld copy AT this frame's
                // cursor, or under the watermark, duplicates an owed-lane
                // delivery — dropped.
                if let Some(held) = held_of(state).get_mut(&topic) {
                    let mut kept = Vec::new();
                    for (seq, withheld) in held.drain(..) {
                        match cursor_of(&withheld) {
                            Some(c) if B::covers(&cursor, &c) => {
                                let below = !B::covers(&c, &cursor);
                                let covered =
                                    watermark.is_some_and(|replayed| B::covers(&replayed, &c));
                                if below && !covered {
                                    out.push(Arc::unwrap_or_clone(withheld));
                                }
                            }
                            _ => kept.push((seq, withheld)),
                        }
                    }
                    *held = kept;
                }
                match state.replayed.get_mut(&topic) {
                    Some(replayed) => B::advance(replayed, cursor),
                    None => {
                        state.replayed.insert(topic.clone(), cursor);
                    }
                }
                out.push((*message).clone());
                // This admission may have completed the owed slice: release
                // the withheld tail right here — ascending, deduped and
                // then covered by the watermark, so a replay re-serving
                // those frames skips them.
                if !state.owed_on(&topic) {
                    let held = held_of(state).remove(&topic).unwrap_or_default();
                    let out = per_lease.entry(*lease).or_default();
                    for (_, withheld) in held {
                        let Some(c) = cursor_of(&withheld) else {
                            out.push(Arc::unwrap_or_clone(withheld));
                            continue;
                        };
                        if state
                            .replayed
                            .get(&topic)
                            .is_some_and(|replayed| B::covers(replayed, &c))
                        {
                            continue;
                        }
                        match state.replayed.get_mut(&topic) {
                            Some(replayed) => B::advance(replayed, c),
                            None => {
                                state.replayed.insert(topic.clone(), c);
                            }
                        }
                        out.push(Arc::unwrap_or_clone(withheld));
                    }
                }
            }
            // Advance the wire position and the wave's per-kind progress —
            // both max-fold, so a deep-history replay below `last_seen`
            // moves neither the siblings' view nor the wire position.
            if let Some(seen) = self.last_seen.get_mut(&topic) {
                B::advance(seen, cursor);
            } else {
                self.last_seen.insert(topic.clone(), cursor);
            }
            if let Some(wave) = wave
                && let Some(pending) = self.pending_waves.get_mut(&wave)
            {
                match pending.progress_mut(kind) {
                    Some(progress) => {
                        if B::covers(progress, &cursor) {
                            // In-wave replay is total cursor order per kind
                            // (XIP-83): a regression is a server order fault.
                            // Telemetry only — max-fold keeps state sane.
                            tracing::warn!(
                                %topic,
                                wave = wave.0,
                                "bidi transport: in-wave {} replay cursor regressed",
                                kind.name()
                            );
                        }
                        B::advance(progress, cursor)
                    }
                    progress @ None => *progress = Some(cursor),
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
    initially_suspended: bool,
    chunk_cap: usize,
    chunk_bytes: usize,
) where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    LedgerTask {
        opener,
        cmds,
        lease_cmds,
        ledger: Ledger {
            chunk_cap,
            chunk_bytes,
            ..Ledger::default()
        },
        conn: None,
        reconnect_delay: RECONNECT_INITIAL_DELAY,
        reconnect_at: tokio::time::Instant::now(),
        wire_opened_at: None,
        suspended: initially_suspended,
        wire_opens: 0,
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
    /// When the current wire opened, or `None` between wires. A wire that dies
    /// having been up at least [`MIN_STABLE_UPTIME`] resets the backoff to the
    /// floor; a shorter-lived one is flapping and keeps it growing.
    wire_opened_at: Option<tokio::time::Instant>,
    /// `suspend()`ed: the wire stays down on purpose — no lazy open for new
    /// leases, no reconnect backoff — until `resume()` clears it.
    suspended: bool,
    /// Wires opened over this transport's lifetime (cold opens + reopens) —
    /// observability only: `wire_opens - 1` is the reconnect count.
    wire_opens: u64,
    /// `resume()` callers awaiting a resume wave that hasn't been sent yet
    /// (the open failed, or hasn't happened). [`Self::reopen`] moves them
    /// onto the resume wave it sends; until then they park here across
    /// backoff retries. When a reconnect has no resume wave to send (every
    /// leased topic rides a re-issue), they park here past the open too and
    /// resolve via [`Self::settle_caught_up_waiters`].
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
        let topics: Vec<Topic> = subs.iter().map(|(topic, _)| topic.clone()).collect();
        let floors: HashMap<Topic, B::Cursor> = subs
            .iter()
            .map(|(topic, cursor)| (topic.clone(), *cursor))
            .collect();
        // One wave per ≤`MAX_MUTATE_TOPICS` chunk, so an add-set over very many
        // topics never rides one oversized frame. Every chunk is owned by this
        // lease, which stays catching-up until the last resolves (`complete`).
        let mut chunks = self.ledger.chunk_waves(subs);

        // A cold open (no wire, no other lease, not suspended) seeds the dial
        // with the first chunk; the rest follow on the outbox. The peeled
        // first chunk is registered as pending below like the others, but its
        // mutate is already on the dial (Opened) or dropped with it
        // (Suspended), so it is never re-queued.
        let cold = self.conn.is_none() && self.ledger.leases.is_empty() && !self.suspended;
        let mut peeled: Option<(WaveId, HashMap<Topic, B::Cursor>)> = None;
        if cold && !chunks.is_empty() {
            let (wave0, claims0, mutate0) = chunks.remove(0);
            peeled = Some((wave0, claims0));
            // The dial is preemptible: commands keep flowing while it runs,
            // and a Suspend drops it. The failure is this caller's to see —
            // nobody else is waiting on the wire.
            match self.open_preemptibly(mutate0).await {
                OpenOutcome::Opened(wire) => {
                    self.conn = Some(wire);
                    // Backoff resets on stable death, not on open: a bare accept
                    // doesn't prove the wire works ([`Self::wire_died`]).
                    self.wire_opened_at = Some(tokio::time::Instant::now());
                    self.wire_opens += 1;
                    tracing::info!(
                        topics = topics.len(),
                        waves = chunks.len() + 1,
                        "bidi transport: wire opened (cold open)"
                    );
                }
                OpenOutcome::Failed(e) => {
                    // The failure also travels to the caller, but join
                    // results are rarely read — the log is the reliable
                    // trace (the reconnect path warns the same way). Nothing
                    // is registered yet, so no orphan lease is left behind.
                    tracing::warn!("bidi cold open failed: {e}");
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
        }

        // Registered for routing immediately, NOT at CatchUpComplete: the
        // lease's own replay arrives *before* its CatchUpComplete — tagged
        // with its wave and routed to it unconditionally — and must reach
        // it. Live frames for an already-live topic also land right away: a
        // no-op re-add joins no wave, so live delivery IS this lease's feed
        // from its cursor on (and whatever a sibling's wave holds back
        // arrives via the sibling-gap rule; module docs).
        let (tx, events) = mpsc::channel(depth.max(1));
        let id = self.ledger.register(topics.clone(), floors.clone(), tx);
        let wave_count = chunks.len() + peeled.is_some() as usize;
        tracing::debug!(
            lease = id.0,
            waves = wave_count,
            topics = topics.len(),
            "bidi transport: lease registered"
        );
        // Every chunk — the peeled first plus the rest — is a pending wave
        // owned by this lease.
        if let Some((wave, claims)) = peeled {
            self.ledger
                .pending_waves
                .insert(wave, PendingWave::new(WaveOwner::Lease(id), claims));
        }
        for (wave, claims, _) in &chunks {
            self.ledger.pending_waves.insert(
                *wave,
                PendingWave::new(WaveOwner::Lease(id), claims.clone()),
            );
        }
        // Queue the remaining chunk mutates iff the wire is up now (cold-opened
        // or already warm), each tagged with the lease so a deref can purge it
        // if it never reaches the wire. When the wire is down (suspended or
        // mid-backoff) every mutate is dropped and the next reopen reissues
        // from the lease's floors.
        if self.conn.is_some() {
            for (wave, _, mutate) in chunks {
                self.outbox.push(Some(id), wave, mutate);
            }
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
        let removes = self.drop_leases(vec![id]);
        self.retire(removes);
        self.settle_idle_waiters();
        self.settle_caught_up_waiters();
        Flow::Continue
    }

    /// Remove leases from the ledger — the seed list plus any that fail a
    /// delivery attempted along the way (a released deferred completion can
    /// hit a wedged channel) — returning the topics left with no holder.
    /// Every removed lease's unsent waves are purged: sent late they would
    /// replay topics from a cursor nobody asked for anymore (siblings would
    /// receive unrequested backlog, and the wave's completion would route to
    /// nobody).
    fn drop_leases(&mut self, mut dropped: Vec<LeaseId>) -> Vec<Topic> {
        let mut removes = Vec::new();
        while let Some(lease) = dropped.pop() {
            let purged = self.outbox.purge(lease);
            removes.extend(self.ledger.deref(lease, &purged, &mut dropped));
        }
        removes
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
        // Pending waves stay, progress intact: their leases are still owed a
        // CatchUpComplete, re-issued alongside the next reopen's resume wave.
        // But a resume already awaiting catch-up is superseded by this suspend
        // — the app is going off the network, so it can never reach its live
        // edge on this transition. Conclude those waiters now instead of
        // stranding them (and their callers' tasks) for the whole background
        // sojourn; the next resume parks a fresh waiter.
        for waiter in self.resume_notify.drain(..) {
            let _ = waiter.send(());
        }
        let _ = reply.send(());
        Flow::Continue
    }

    /// `Cmd::Resume`: back onto the network; the reply resolves once a live
    /// wire has no wave left in flight ("catch up, then done" — against
    /// everything owed, not just a resume wave: an interrupted lease wave's
    /// re-issue is exactly as much of the catch-up as the resume wave).
    async fn resume(&mut self, reply: oneshot::Sender<()>) -> Flow {
        tracing::info!(
            leases = self.ledger.leases.len(),
            "bidi transport: resuming — catch up, then done"
        );
        self.suspended = false;
        // Every waiter parks the same way and resolves the same way
        // ([`Self::settle_caught_up_waiters`]); the settle calls below catch
        // the already-satisfied cases immediately.
        self.resume_notify.push(reply);
        if self.conn.is_some() {
            self.settle_caught_up_waiters();
            return Flow::Continue;
        }
        if self.ledger.leases.is_empty() {
            self.settle_idle_waiters();
            return Flow::Continue;
        }
        self.reconnect_delay = RECONNECT_INITIAL_DELAY;
        self.reconnect().await
    }

    /// A wire event: route it, and drop any lease that failed delivery.
    fn wire_event(&mut self, event: Event<B::GroupMessage, B::WelcomeMessage>) -> Flow {
        if let Event::CatchUpComplete { mutate_id: 0 } = event {
            // A tag-serving backend echoes the Mutate's `mutate_id`, and ours
            // are minted from 1 — every wave, including the uncorrelated
            // removes-only waves ([`LedgerTask::retire`]), whose always-ack
            // echo would otherwise arrive as `0`. So `0` here can only be a
            // pre-delivery-tags backend whose responses lack the field. This
            // ledger cannot run on one:
            // its untagged replays are dropped by the live-order shield
            // (partial delivery) and its completions resolve no wave, so
            // fail fast instead — shut down, which ends every lease and makes
            // every later `lease()` return `Closed`, the same tombstone an
            // unretryable re-open leaves. The streams that just ended replay
            // from durable cursors on the legacy path (dispatch latches on
            // the `Closed`), so nothing the shield dropped is lost.
            tracing::error!(
                "bidi transport: backend serves untagged frames (pre-delivery-tags node); \
                 closing so streams fall back to the legacy path"
            );
            return self.shutdown();
        }
        let dropped = self.ledger.route(event);
        let removes = self.drop_leases(dropped);
        self.retire(removes);
        self.settle_idle_waiters();
        self.settle_caught_up_waiters();
        Flow::Continue
    }

    /// Arm the next reconnect at a jittered deadline and return the chosen
    /// delay (for logging). The exponential base ([`Self::reconnect_delay`])
    /// is the floor; a random offset of up to one more base spreads the
    /// attempt across `[delay, 2·delay)`. A server restart releases every
    /// client's wire in the same instant, so without this spread the whole
    /// fleet would reconnect in lockstep and re-stampede the backend on each
    /// backoff step — the jitter de-syncs them (same rationale as the stream
    /// watchdog's `XMTP_STREAM_WATCHDOG_RECONNECT_JITTER_MS`).
    fn arm_reconnect(&mut self) -> std::time::Duration {
        let delay = self.reconnect_delay + xmtp_common::time::rand_offset(self.reconnect_delay);
        self.reconnect_at = tokio::time::Instant::now() + delay;
        delay
    }

    /// The wire ended (server close or transport failure). Leases survive:
    /// the dead-wire select arm reconnects with a resume wave (module docs)
    /// — no stream ever observes the flap.
    fn wire_died(&mut self) -> Flow {
        self.outbox.clear();
        drop(self.conn.take());
        // A wire that proved stable resets the backoff to the floor so the
        // reconnect is prompt; one that died inside `MIN_STABLE_UPTIME` is
        // flapping — grow the backoff so an accept-then-close server can't pin
        // us at the floor. Only surviving resets it; a bare open never did.
        let stable = self
            .wire_opened_at
            .take()
            .is_some_and(|opened| opened.elapsed() >= MIN_STABLE_UPTIME);
        self.reconnect_delay = if stable {
            RECONNECT_INITIAL_DELAY
        } else {
            (self.reconnect_delay * 2).min(RECONNECT_MAX_DELAY)
        };
        let retry_in = self.arm_reconnect();
        if !self.ledger.leases.is_empty() {
            tracing::warn!(
                leases = self.ledger.leases.len(),
                interrupted_waves = self.ledger.pending_waves.len(),
                retry_in_ms = retry_in.as_millis() as u64,
                "bidi transport: wire died; reconnecting from last-seen positions"
            );
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
        // Move any dial-deferred resumes into `resume_notify`, then conclude
        // every parked resume: this suspend supersedes them all (same reason
        // as [`Self::suspend`]).
        self.park_deferred_resumes();
        for waiter in self.resume_notify.drain(..) {
            let _ = waiter.send(());
        }
        let _ = ack.send(());
    }

    /// Open a fresh wire and rebuild every open obligation on it (module
    /// docs): a resume wave re-adds every topic no interrupted wave owns,
    /// from `last_seen`; each interrupted lease wave re-issues under a fresh
    /// wave id at the plan's per-topic cursors. The resume wave rides the
    /// open as the initial frame; re-issued mutates follow via the outbox;
    /// `resume()` waiters stay parked in `resume_notify` until no wave is
    /// left in flight. On failure the backoff grows and every obligation —
    /// waiters, waves, progress — stays put for the next attempt. The dial
    /// is preemptible ([`Self::open_preemptibly`]): commands keep flowing
    /// while it runs, so a `suspend()` never waits on a stuck dial.
    async fn reopen(&mut self) -> AfterReopen {
        let Some(ReconnectPlan {
            resume_adds,
            reissues,
            orphans,
        }) = self.ledger.reconnect_plan()
        else {
            // Nothing leased: there is nothing to catch up on, so any
            // resume() waiters are already done.
            for waiter in self.resume_notify.drain(..) {
                let _ = waiter.send(());
            }
            return AfterReopen::Proceed;
        };
        // When every leased topic is owned by an interrupted wave there is no
        // resume wave (an adds-nothing wave is refused by the wire contract)
        // and the first re-issue seeds the open instead.
        let resumed_topics = resume_adds.len();
        let mut outbound = std::collections::VecDeque::new();
        // Every open obligation is chunked to ≤`MAX_MUTATE_TOPICS` topics per
        // frame, so resuming an agent's whole subscription set never rides one
        // oversized `Mutate`. Resume chunks carry no owner lease (their replay
        // reaches holders via the sibling-gap rule); each interrupted lease's
        // re-issue chunks stay owned by that lease, so `complete` holds it
        // until its last chunk lands.
        let mut new_waves: Vec<(WaveId, PendingWave<B>)> = Vec::new();
        for (wave, claims, mutate) in self.ledger.chunk_waves(resume_adds) {
            outbound.push_back((None, wave, mutate));
            new_waves.push((wave, PendingWave::new(WaveOwner::Resume, claims)));
        }
        for reissue in reissues {
            let lease = reissue.lease;
            for (wave, claims, mutate) in self.ledger.chunk_waves(reissue.adds) {
                outbound.push_back((Some(lease), wave, mutate));
                new_waves.push((wave, PendingWave::new(WaveOwner::Lease(lease), claims)));
            }
        }
        let Some((_, _, initial)) = outbound.pop_front() else {
            // Unreachable — every leased topic is either in the resume adds
            // or owned by a re-issue — but never send an adds-nothing frame;
            // retry on the normal backoff instead.
            self.arm_reconnect();
            return AfterReopen::Proceed;
        };
        match self.open_preemptibly(initial).await {
            OpenOutcome::Opened(wire) => {
                self.conn = Some(wire);
                // Backoff resets on stable death, not on open ([`wire_died`]).
                self.wire_opened_at = Some(tokio::time::Instant::now());
                self.wire_opens += 1;
                tracing::info!(
                    reconnects = self.wire_opens.saturating_sub(1),
                    resumed_topics,
                    waves = new_waves.len(),
                    "bidi transport: wire reopened"
                );
                // Waves from the dead wire can never resolve on this one —
                // drop them; the re-issues re-home the interrupted leases'
                // obligations (progress carried forward). Completions that
                // were parked on a dropped wave re-check below, once the new
                // waves are in: they re-park on whichever new wave claimed
                // their topic (as do the orphans, whose entire re-issue was
                // subsumed by a sibling's lower re-add). `resume()` waiters
                // stay in `resume_notify` and resolve when no wave is left
                // in flight ([`Self::settle_caught_up_waiters`]).
                let mut released = orphans;
                for (_, wave) in self.ledger.pending_waves.drain() {
                    released.extend(wave.holds);
                }
                for (wave, pending) in new_waves {
                    self.ledger.pending_waves.insert(wave, pending);
                }
                // Re-issued mutates follow the initial frame in issue order,
                // each tagged with its lease so a deref purges it.
                for (lease, wave, mutate) in outbound {
                    self.outbox.push(lease, wave, mutate);
                }
                let mut dropped = Vec::new();
                for lease in released {
                    self.ledger.complete(lease, &mut dropped);
                }
                let removes = self.drop_leases(dropped);
                self.retire(removes);
                // The released completions can have dropped the last lease
                // (wedged channel): the settles below keep parked resume()
                // waiters from hanging on a dormant task, exactly as after
                // a deref or wire event.
                self.settle_idle_waiters();
                self.settle_caught_up_waiters();
                AfterReopen::Proceed
            }
            OpenOutcome::Failed(e) => {
                if !e.is_retryable() {
                    // The backend refuses the surface outright — no redial
                    // can succeed. Shutting the ledger down ends every lease,
                    // so consumers see their streams end and every later
                    // `lease()` fails with `Closed` — the refusal's
                    // tombstone.
                    tracing::error!("bidi transport: reconnect refused ({e}); closing");
                    return AfterReopen::Shutdown;
                }
                self.reconnect_delay = (self.reconnect_delay * 2).min(RECONNECT_MAX_DELAY);
                let retry_in = self.arm_reconnect();
                tracing::warn!(
                    retry_in_ms = retry_in.as_millis() as u64,
                    "bidi transport: reconnect failed ({e}); backing off"
                );
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
    /// parked `resume()` waiter and drop the wave obligations, which can
    /// never resolve (their wire is gone or going, and nothing will re-open
    /// it). Called whenever a deref or wire death may have emptied the
    /// ledger; a no-op while any lease remains.
    fn settle_idle_waiters(&mut self) {
        if !self.ledger.leases.is_empty() {
            return;
        }
        self.ledger.pending_waves.clear();
        for waiter in self.resume_notify.drain(..) {
            let _ = waiter.send(());
        }
    }

    /// Where every `resume()` waiter resolves: a live wire with no wave left
    /// in flight is caught up against everything owed — the resume wave, the
    /// re-issues, and any deferred completions parked on them. (A wave
    /// issued meanwhile — a fresh lease's adds — extends the wait, but its
    /// completion still requires the live edge; "catch up, then done"
    /// holds.)
    fn settle_caught_up_waiters(&mut self) {
        if self.conn.is_none() || !self.ledger.pending_waves.is_empty() {
            return;
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
        // Chunked like add-waves ([`Ledger::chunk_waves`]): a mass unsubscribe
        // — dropping a lease over very many topics — must not ride one
        // oversized frame either. Each removes wave's id is minted, not `0`:
        // the server acks EVERY Mutate (always-ack), echoing the id back. The
        // ledger ignores the ack — the id is in no pending wave, deliberately:
        // a removes wave carries no obligation — but it must not be `0`, or the
        // echo would be indistinguishable from a pre-tags backend's empty
        // `CatchUpComplete` and trip the untagged-backend tombstone in
        // [`LedgerTask::wire_event`].
        let groups = chunk_by_budget(
            removes,
            self.ledger.chunk_cap,
            self.ledger.chunk_bytes,
            topic_wire_cost,
        );
        for group in groups {
            let wave = self.ledger.next_wave_id();
            self.outbox
                .push(None, wave, B::build_mutate(None, group, wave.0));
        }
    }

    /// Push waves onto the wire until it's momentarily full, closed, or the
    /// outbox is empty. Never blocks; `Full` leaves the wave at the front
    /// for the next flush, `Closed` is left for the select's `Wire(None)` to
    /// clean up.
    fn flush_outbox(&mut self) {
        let Some(wire) = self.conn.as_ref() else {
            return;
        };
        while let Some((lease, id, wave)) = self.outbox.waves.pop_front() {
            match wire.try_mutate(wave) {
                Ok(()) => {}
                Err(TryMutateError::Full(wave)) | Err(TryMutateError::Closed(wave)) => {
                    self.outbox.waves.push_front((lease, id, wave));
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
    waves: std::collections::VecDeque<(Option<LeaseId>, WaveId, M)>,
}

impl<M> Default for Outbox<M> {
    fn default() -> Self {
        Self {
            waves: std::collections::VecDeque::new(),
        }
    }
}

impl<M> Outbox<M> {
    fn push(&mut self, lease: Option<LeaseId>, id: WaveId, wave: M) {
        self.waves.push_back((lease, id, wave));
    }

    fn is_empty(&self) -> bool {
        self.waves.is_empty()
    }

    fn clear(&mut self) {
        self.waves.clear();
    }

    /// Drop every unsent wave issued by `lease` — its deref makes them
    /// stale — and return their ids: the ledger forgets exactly these. A
    /// wave NOT returned here already reached the wire, so it lives on the
    /// server and must stay in the ledger until its `CatchUpComplete`.
    fn purge(&mut self, lease: LeaseId) -> Vec<WaveId> {
        let purged = self
            .waves
            .iter()
            .filter(|(owner, ..)| *owner == Some(lease))
            .map(|(_, id, _)| *id)
            .collect();
        self.waves.retain(|(owner, ..)| *owner != Some(lease));
        purged
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
        transport_born(false)
    }

    /// [`transport`], with the birth state explicit.
    fn transport_born(suspended: bool) -> (BidiTransport<V3Binding>, Servers) {
        let servers: Servers = Arc::default();
        let sink = servers.clone();
        let transport = BidiTransport::new(
            move |initial| {
                let (api, server) = mock_pair();
                sink.lock().unwrap().push(server);
                async move {
                    BidiConnection::open(&api, initial)
                        .await
                        .map_err(OpenError::new)
                }
            },
            suspended,
        );
        (transport, servers)
    }

    /// [`transport`] with the chunk limits shrunk, to force byte- or
    /// count-based splitting on a handful of small topics.
    fn transport_capped(
        max_topics: usize,
        max_bytes: usize,
    ) -> (BidiTransport<V3Binding>, Servers) {
        let servers: Servers = Arc::default();
        let sink = servers.clone();
        let transport = BidiTransport::new_with_chunk_limits(
            move |initial| {
                let (api, server) = mock_pair();
                sink.lock().unwrap().push(server);
                async move {
                    BidiConnection::open(&api, initial)
                        .await
                        .map_err(OpenError::new)
                }
            },
            false,
            max_topics,
            max_bytes,
        );
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

    /// One frame of a wave's catch-up replay, stamped with the wave's
    /// `mutate_id` — like the real tagged server, which tags every delivery
    /// frame with the wave that produced it and never mixes lanes (or two
    /// waves) in one frame.
    fn replay(
        mutate_id: u64,
        group: Vec<GroupMessage>,
        welcome: Vec<WelcomeMessage>,
    ) -> subscribe_response::v1::Response {
        subscribe_response::v1::Response::Messages(subscribe_response::v1::Messages {
            group_messages: group,
            welcome_messages: welcome,
            mutate_id,
        })
    }

    /// A live-lane delivery frame (tag 0). Scripts may play one only for
    /// topics no in-flight wave owns — a wave's topics receive no live frames
    /// until its `CatchUpComplete` (the server guarantee the routing rules
    /// are built on).
    fn live(
        group: Vec<GroupMessage>,
        welcome: Vec<WelcomeMessage>,
    ) -> subscribe_response::v1::Response {
        replay(0, group, welcome)
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

    /// A lease over more than [`MAX_MUTATE_TOPICS`] topics is split across
    /// several `Mutate` frames, none exceeding the cap, so no single frame can
    /// approach the encode limit. Each chunk is its own wave, and the lease is
    /// not caught up until its LAST chunk completes — completing only the
    /// first leaves it deferred.
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_lease_over_the_cap_splits_into_bounded_frames() {
        let (transport, servers) = transport();
        let topics: Vec<(Topic, u64)> = (0..(MAX_MUTATE_TOPICS as u32 + 1))
            .map(|i| (group_topic(&i.to_le_bytes()), 0))
            .collect();
        let mut lease = transport.lease(topics, DEFAULT_LEASE_DEPTH).await?;

        let mut server = take_server(&servers);
        // Cold open rides the first chunk; the overflow follows on the outbox.
        let first = server.next_mutate().await;
        let second = server.next_mutate().await;
        assert_eq!(first.adds.len(), MAX_MUTATE_TOPICS, "first frame is capped");
        assert_eq!(second.adds.len(), 1, "the overflow rides a second frame");
        assert_ne!(
            first.mutate_id, second.mutate_id,
            "each chunk is its own correlatable wave"
        );

        // Completing only the first chunk must NOT catch the lease up.
        server.send(catchup_complete(first.mutate_id));
        let quiet = tokio::time::timeout(Duration::from_millis(200), lease.next()).await;
        assert!(
            quiet.is_err(),
            "a chunked lease must not report caught-up until its last chunk completes"
        );

        // The last chunk's completion catches it up.
        server.send(catchup_complete(second.mutate_id));
        assert!(
            matches!(recv(&mut lease).await, Some(LeaseEvent::CatchUpComplete)),
            "the last chunk's completion catches the lease up"
        );
    }

    /// A reconnect resuming more than [`MAX_MUTATE_TOPICS`] topics chunks the
    /// resume wave the same way — an agent's whole subscription set never
    /// rides one oversized frame on reconnect.
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_reconnect_resume_over_the_cap_splits_into_bounded_frames() {
        let (transport, servers) = transport();
        let topics: Vec<(Topic, u64)> = (0..(MAX_MUTATE_TOPICS as u32 + 1))
            .map(|i| (group_topic(&i.to_le_bytes()), 0))
            .collect();
        let mut lease = transport.lease(topics, DEFAULT_LEASE_DEPTH).await?;

        let mut server = take_server(&servers);
        // Catch both cold-open chunks up so every topic is live on the wire.
        let first = server.next_mutate().await;
        let second = server.next_mutate().await;
        server.send(catchup_complete(first.mutate_id));
        server.send(catchup_complete(second.mutate_id));
        assert!(matches!(
            recv(&mut lease).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // The wire dies; the reconnect re-adds every leased topic, and that
        // resume is itself chunked.
        drop(server);
        let mut reconnect = wait_for_server(&servers).await;
        let resume_a = reconnect.next_mutate().await;
        let resume_b = reconnect.next_mutate().await;
        assert_eq!(
            resume_a.adds.len(),
            MAX_MUTATE_TOPICS,
            "resume frame is capped"
        );
        assert_eq!(
            resume_b.adds.len(),
            1,
            "the overflow rides a second resume frame"
        );
        assert_ne!(
            resume_a.mutate_id, resume_b.mutate_id,
            "each resume chunk is its own wave"
        );
    }

    /// A lease whose topics exceed the BYTE budget (not the count cap) still
    /// splits into bounded frames — the guarantee holds regardless of topic
    /// identifier length, not just topic count.
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_lease_over_the_byte_budget_splits_into_bounded_frames() {
        // Count cap is effectively off, so only the byte budget bites; the
        // budget is exactly two topics' worth, so each frame holds two.
        let per_topic = topic_wire_cost(&group_topic(&0u32.to_le_bytes()));
        let (transport, servers) = transport_capped(usize::MAX, 2 * per_topic);
        let topics: Vec<(Topic, u64)> = (0..5u32)
            .map(|i| (group_topic(&i.to_le_bytes()), 0))
            .collect();
        let _lease = transport.lease(topics, DEFAULT_LEASE_DEPTH).await?;

        let mut server = take_server(&servers);
        // Read frames until every topic is subscribed; each frame stays within
        // the budget (≤ 2 adds here).
        let mut subscribed = 0;
        while subscribed < 5 {
            let mutate = server.next_mutate().await;
            assert!(
                !mutate.adds.is_empty() && mutate.adds.len() <= 2,
                "a byte-budgeted frame holds 1–2 adds, got {}",
                mutate.adds.len()
            );
            subscribed += mutate.adds.len();
        }
        assert_eq!(subscribed, 5, "every topic is subscribed across the frames");
    }

    /// Dropping a lease over more topics than the cap retires them in chunked
    /// removes waves — a mass unsubscribe is bounded just like an add-wave. A
    /// keeper lease holds the wire open, so the drop retires the topics rather
    /// than closing the wire (which is what dropping the *last* lease does).
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_mass_unsubscribe_chunks_the_removes_wave() {
        let (transport, servers) = transport_capped(2, usize::MAX);
        let keeper = transport
            .lease(vec![(group_topic(b"keep"), 0)], DEFAULT_LEASE_DEPTH)
            .await?;
        let mut server = take_server(&servers);
        let k = server.next_mutate().await;
        server.send(catchup_complete(k.mutate_id));

        // A big lease over three topics unique to it, chunked into two
        // add-waves (2 + 1) at the count cap; catch both up so they are live.
        let big = transport
            .lease(
                (0..3u32)
                    .map(|i| (group_topic(&i.to_le_bytes()), 0))
                    .collect(),
                DEFAULT_LEASE_DEPTH,
            )
            .await?;
        let a = server.next_mutate().await;
        let b = server.next_mutate().await;
        server.send(catchup_complete(a.mutate_id));
        server.send(catchup_complete(b.mutate_id));

        // Drop the big lease; its three topics have no other holder and retire,
        // and the removes wave is chunked at the same cap → 2 + 1.
        drop(big);
        let mut removed = 0;
        while removed < 3 {
            let mutate = server.next_mutate().await;
            if mutate.removes.is_empty() {
                continue; // ignore a trailing add/echo frame
            }
            assert!(
                mutate.removes.len() <= 2,
                "each removes frame respects the cap, got {}",
                mutate.removes.len()
            );
            removed += mutate.removes.len();
        }
        assert_eq!(removed, 3, "every dropped topic is unsubscribed");
        drop(keeper);
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

    /// A live frame fans out by topic: group and welcome deliveries land only
    /// on the leases holding their topic, in wire order; an unroutable
    /// message is skipped, not fatal.
    #[xmtp_common::test(unwrap_try = true)]
    async fn deliveries_demux_by_topic() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut beta = transport.lease(vec![(group_topic(b"g2"), 0)], 8).await?;
        let mut inst = transport.lease(vec![(welcome_topic(b"i1"), 0)], 8).await?;
        let mut server = take_server(&servers);

        // Complete every add wave first: only then may all three topics carry
        // live frames.
        for _ in 0..3 {
            let wave = server.next_mutate().await;
            server.send(catchup_complete(wave.mutate_id));
        }
        for lease in [&mut alpha, &mut beta, &mut inst] {
            assert!(matches!(
                recv(lease).await,
                Some(LeaseEvent::CatchUpComplete)
            ));
        }

        let (m1, m2, m3) = (
            group_msg(1, b"g1"),
            group_msg(2, b"g2"),
            group_msg(3, b"g1"),
        );
        let unroutable = GroupMessage { version: None };
        let w = welcome_msg(9, b"i1");
        server.send(live(
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

    /// The sibling-gap rule: a lease re-adding a shared topic at a lower
    /// cursor moves the topic into its wave, so the other holders' only
    /// source for the gapped segment is that wave's tagged frames — the
    /// ledger forwards them exactly the `> last_seen` part. Above the gap
    /// reaches BOTH; below it reaches only the re-adder, so a caught-up
    /// lease receives each message exactly once.
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_siblings_replay_fills_the_gap_without_repeating_history() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;

        server.send(replay(
            first.mutate_id,
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

        // A sibling re-adds from zero — the topic moves into its wave (live
        // delivery stops for alpha too) and the wave replays 1..2 plus a 3
        // that arrived mid-wave, all tagged with the sibling's wave.
        let mut beta = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let second = server.next_mutate().await;
        server.send(replay(
            second.mutate_id,
            vec![
                group_msg(1, b"g1"),
                group_msg(2, b"g1"),
                group_msg(3, b"g1"),
            ],
            vec![],
        ));
        server.send(catchup_complete(second.mutate_id));

        // The wave's owner gets the full replay unconditionally...
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
        // ...the caught-up sibling only the part past `last_seen` — the gap
        // live delivery would have given it.
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(
                got,
                vec![group_msg(3, b"g1")],
                "below last_seen is the owner's replay alone; above it is the sibling gap"
            ),
            _ => panic!("alpha expected only the new message"),
        }
    }

    /// Tag routing is unconditional for the owner and closed to everyone
    /// else below `last_seen`: a deep re-add's replay is history the wire
    /// has already seen, so the owner lease receives all of it (its replay,
    /// exactly once) while a caught-up sibling on the same topic receives
    /// none of it.
    #[xmtp_common::test(unwrap_try = true)]
    async fn tagged_replay_below_last_seen_is_the_owners_alone() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(replay(
            first.mutate_id,
            vec![
                group_msg(1, b"g1"),
                group_msg(2, b"g1"),
                group_msg(3, b"g1"),
            ],
            vec![],
        ));
        server.send(catchup_complete(first.mutate_id));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got.len(), 3),
            _ => panic!("alpha expected its replay"),
        }
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // A sibling re-adds from zero; its wave's replay sits entirely at or
        // below `last_seen` (3).
        let mut beta = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let second = server.next_mutate().await;
        server.send(replay(
            second.mutate_id,
            vec![
                group_msg(1, b"g1"),
                group_msg(2, b"g1"),
                group_msg(3, b"g1"),
            ],
            vec![],
        ));
        server.send(catchup_complete(second.mutate_id));
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(
                got,
                vec![
                    group_msg(1, b"g1"),
                    group_msg(2, b"g1"),
                    group_msg(3, b"g1")
                ],
                "the owner's replay is admitted even below last_seen"
            ),
            _ => panic!("beta expected its full replay"),
        }
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // Alpha saw none of that replay: its very next event is the live
        // frame after the wave — nothing was queued in between.
        server.send(live(vec![group_msg(4, b"g1")], vec![]));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(
                got,
                vec![group_msg(4, b"g1")],
                "a sibling's below-last_seen replay must never reach a caught-up lease"
            ),
            _ => panic!("alpha expected only the live message"),
        }
    }

    /// Live frames are total-cursor-ordered and never overlap a wave's
    /// replay, so a live frame at or below `last_seen` is a server fault:
    /// the shield drops it instead of handing every holder a duplicate.
    #[xmtp_common::test(unwrap_try = true)]
    async fn covered_live_frame_is_dropped() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(replay(
            first.mutate_id,
            vec![group_msg(1, b"g1"), group_msg(2, b"g1")],
            vec![],
        ));
        server.send(catchup_complete(first.mutate_id));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got.len(), 2),
            _ => panic!("alpha expected its replay"),
        }
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // A faulty live frame repeats 2 alongside the genuinely new 3.
        server.send(live(vec![group_msg(2, b"g1"), group_msg(3, b"g1")], vec![]));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(
                got,
                vec![group_msg(3, b"g1")],
                "a covered live frame must be dropped, not re-delivered"
            ),
            _ => panic!("alpha expected only the fresh live message"),
        }
    }

    /// A topic held by two leases fans its live deliveries out to both.
    #[xmtp_common::test(unwrap_try = true)]
    async fn shared_topic_fans_out_to_every_lease() {
        let (transport, servers) = transport();
        let shared = group_topic(b"g1");
        let mut alpha = transport.lease(vec![(shared.clone(), 0)], 8).await?;
        let mut beta = transport.lease(vec![(shared.clone(), 0)], 8).await?;
        let mut server = take_server(&servers);

        // Both add waves complete (empty topic) before the live frame.
        for _ in 0..2 {
            let wave = server.next_mutate().await;
            server.send(catchup_complete(wave.mutate_id));
        }
        let m = group_msg(1, b"g1");
        server.send(live(vec![m.clone()], vec![]));

        for lease in [&mut alpha, &mut beta] {
            assert!(matches!(
                recv(lease).await,
                Some(LeaseEvent::CatchUpComplete)
            ));
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
        let mut server = take_server(&servers);
        let slow_wave = server.next_mutate().await;
        // Fast is owed nothing (empty ack) — its feed is pure fresh fan-out.
        let fast_wave = server.next_mutate().await;
        server.send(catchup_complete(fast_wave.mutate_id));
        assert!(matches!(
            recv(&mut fast).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // Slow's wave replays in three frames; the owner takes them by tag,
        // the caught-up sibling past `last_seen` — every frame reaches both.
        let (m1, m2, m3) = (
            group_msg(1, b"g1"),
            group_msg(2, b"g1"),
            group_msg(3, b"g1"),
        );
        server.send(replay(slow_wave.mutate_id, vec![m1.clone()], vec![]));
        server.send(replay(slow_wave.mutate_id, vec![m2.clone()], vec![]));
        server.send(replay(slow_wave.mutate_id, vec![m3.clone()], vec![]));

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
        server.send(replay(
            first.mutate_id,
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
        assert_eq!(resume.adds.len(), 1, "one resume wave");
        assert_eq!(resume.adds[0].topic, group_topic(b"g1").cloned_vec());
        assert_eq!(
            resume.adds[0].id_cursor, 2,
            "resume from last-seen, not the lease floor"
        );

        // The lease never ended: a defensive server overlap in the resume
        // wave's replay is filtered (2 is covered by last_seen), new flows.
        second.send(replay(
            resume.mutate_id,
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
        second.send(live(vec![group_msg(1, b"g1")], vec![]));

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
        server.send(replay(first.mutate_id, vec![group_msg(1, b"g1")], vec![]));
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

        // The lease rides straight onto the new wire: the resume wave's
        // tagged replay reaches every holder past `last_seen`.
        second.send(replay(resume.mutate_id, vec![group_msg(2, b"g1")], vec![]));
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
        server.send(replay(first.mutate_id, vec![group_msg(1, b"g1")], vec![]));
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
        second.send(live(vec![group_msg(2, b"g1")], vec![]));
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
    /// backoff, no lazy open for new leases — and `resume()` covers both
    /// leases with one open: the caught-up lease's topic rides the resume
    /// wave, the suspended-time lease's adds re-issue as their own wave on
    /// the same wire (its catch-up was interrupted before it ever reached
    /// the network).
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
        // The resume wave (the open's initial frame) re-adds the caught-up
        // lease's topic; the suspended-time lease's re-issued wave follows.
        let resume = second.next_mutate().await;
        assert_eq!(
            resume.adds.iter().map(|add| &add.topic).collect::<Vec<_>>(),
            vec![&group_topic(b"g1").cloned_vec()],
            "the resume wave covers only the topic no pending wave owns"
        );
        let reissue = second.next_mutate().await;
        assert_eq!(
            reissue
                .adds
                .iter()
                .map(|add| &add.topic)
                .collect::<Vec<_>>(),
            vec![&group_topic(b"g2").cloned_vec()],
            "the suspended-time lease re-issues as its own wave"
        );
        assert!(
            servers.lock().unwrap().is_empty(),
            "one wire serves both leases"
        );

        // The resume wave's completion alone must NOT resolve the resume()
        // caller: the suspended-time lease's re-issue is as much of the
        // catch-up as the resume wave ("catch up, then done" spans
        // everything owed).
        second.send(catchup_complete(resume.mutate_id));
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            !resumed.is_finished(),
            "resume() must wait for the re-issued wave too"
        );

        second.send(catchup_complete(reissue.mutate_id));
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        tokio::time::timeout(WAIT, resumed).await?.unwrap()?;
    }

    /// A transport born suspended — the suspend intent predates the
    /// transport, an app launched into the background — parks its first
    /// lease instead of dialing; `resume()` opens the wire, the parked
    /// lease's wave rides the open to catch-up, and live delivery follows.
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_born_suspended_transport_parks_the_first_lease() {
        let (transport, servers) = transport_born(true);

        // The first lease registers but must not dial.
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        // Well past several reconnect backoffs: still nothing on the network.
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert!(
            servers.lock().unwrap().is_empty(),
            "a born-suspended transport must keep its first lease parked"
        );

        let resumed = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });
        let mut server = wait_for_server(&servers).await;
        // Every leased topic is owned by the parked wave, so the open's
        // first frame is that wave itself — no resume wave precedes it.
        let wave = server.next_mutate().await;
        assert_eq!(
            wave.adds.iter().map(|add| &add.topic).collect::<Vec<_>>(),
            vec![&group_topic(b"g1").cloned_vec()],
            "the parked lease's adds open the wire"
        );
        server.send(catchup_complete(wave.mutate_id));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        tokio::time::timeout(WAIT, resumed).await?.unwrap()?;

        server.send(live(vec![group_msg(1, b"g1")], vec![]));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::GroupMessages(_))
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
            BidiTransport::new(
                move |initial| {
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
                            .map_err(OpenError::new)
                    }
                },
                false,
            )
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
            BidiTransport::new(
                move |initial| {
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
                            .map_err(OpenError::new)
                    }
                },
                false,
            )
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
        // The deferred resume was superseded by the suspend: its waiter
        // concludes now, not stranded until some later resume.
        tokio::time::timeout(WAIT, first_resume).await?.unwrap()?;
        let mut alpha = tokio::time::timeout(WAIT, leased).await?.unwrap()?;

        // The stale resume must not resurrect the network.
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert_eq!(
            dials.load(Ordering::SeqCst),
            1,
            "a resume deferred before the suspend must not redial"
        );

        // A later, explicit resume brings the wire back.
        let second_resume = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });
        let mut server = wait_for_server(&servers).await;
        let resume = server.next_mutate().await;
        server.send(catchup_complete(resume.mutate_id));
        tokio::time::timeout(WAIT, second_resume).await?.unwrap()?;
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
            BidiTransport::new(
                move |initial| {
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
                            return Err(OpenError::retryable(std::io::Error::other("down")));
                        }
                        if n >= 2 && network_down.load(Ordering::SeqCst) {
                            return Err(OpenError::retryable(std::io::Error::other("down")));
                        }
                        let (api, server) = mock_pair();
                        sink.lock().unwrap().push(server);
                        BidiConnection::open(&api, initial)
                            .await
                            .map_err(OpenError::new)
                    }
                },
                false,
            )
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
        server.send(live(vec![group_msg(1, b"g1")], vec![]));
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
        server.send(live(vec![group_msg(1, b"g1")], vec![]));
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
    /// its interrupted wave re-issues under a fresh wave id from its floor
    /// (no progress was made), owns its topic — so no resume wave is sent —
    /// and the re-issued wave's CatchUpComplete resolves its pending marker.
    #[xmtp_common::test(unwrap_try = true)]
    async fn interrupted_catch_up_is_re_owed_by_a_reissued_wave() {
        let (transport, servers) = transport();
        let mut caught = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(replay(first.mutate_id, vec![group_msg(8, b"g1")], vec![]));
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
        let interrupted = server.next_mutate().await;
        drop(server);

        // The re-issue starts from the pending lease's floor (5), not
        // last-seen (8): its interrupted replay is still owed. And it is a
        // FRESH wave — the dead wire's id can never resolve on this one.
        let mut second = wait_for_server(&servers).await;
        let reissue = second.next_mutate().await;
        assert_eq!(reissue.adds[0].id_cursor, 5);
        assert_ne!(
            reissue.mutate_id, interrupted.mutate_id,
            "a re-issue is a fresh wave"
        );

        second.send(replay(
            reissue.mutate_id,
            vec![group_msg(6, b"g1"), group_msg(8, b"g1")],
            vec![],
        ));
        second.send(catchup_complete(reissue.mutate_id));

        // The pending lease gets its replay AND its marker from the
        // re-issued wave...
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
        // ...while the caught-up lease skips the re-issued replay entirely
        // (all of it is at or below `last_seen`).
        second.send(live(vec![group_msg(9, b"g1")], vec![]));
        match recv(&mut caught).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(
                got,
                vec![group_msg(9, b"g1")],
                "the caught-up lease must skip the re-issued replay entirely"
            ),
            _ => panic!("caught-up lease expected only the new message"),
        }
    }

    /// A wave interrupted mid-replay re-issues past its per-kind progress,
    /// not from its floor: the reconnect sends one resume wave for the live
    /// topic at `last_seen` plus one fresh wave re-adding the interrupted
    /// lease's topic at max(floor, progress), and the re-issued wave's
    /// replay-then-CatchUpComplete completes the lease without repeating
    /// what the dead wire already delivered.
    #[xmtp_common::test(unwrap_try = true)]
    async fn interrupted_wave_reissues_past_its_progress_on_reconnect() {
        let (transport, servers) = transport();
        // One topic goes fully live, pinning the resume side of the plan.
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(replay(first.mutate_id, vec![group_msg(2, b"g1")], vec![]));
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // Beta's wave delivers part of its replay (progress reaches 12) —
        // then the wire dies before its CatchUpComplete.
        let mut beta = transport.lease(vec![(group_topic(b"g2"), 10)], 8).await?;
        let interrupted = server.next_mutate().await;
        server.send(replay(
            interrupted.mutate_id,
            vec![group_msg(11, b"g2"), group_msg(12, b"g2")],
            vec![],
        ));
        // Confirm the ledger routed (and so recorded) the partial replay
        // before killing the wire.
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(11, b"g2"), group_msg(12, b"g2")])
            }
            _ => panic!("beta expected its partial replay"),
        }
        drop(server);

        // The resume wave (the fresh wire's initial frame) re-adds only the
        // live topic, at `last_seen`; the re-issue follows, pushed past the
        // progress the dead wire already delivered — not beta's floor (10).
        let mut second = wait_for_server(&servers).await;
        let resume = second.next_mutate().await;
        assert_eq!(
            resume
                .adds
                .iter()
                .map(|add| (add.topic.clone(), add.id_cursor))
                .collect::<Vec<_>>(),
            vec![(group_topic(b"g1").cloned_vec(), 2)],
            "the resume wave covers only topics no interrupted wave owns"
        );
        let reissue = second.next_mutate().await;
        assert_eq!(
            reissue
                .adds
                .iter()
                .map(|add| (add.topic.clone(), add.id_cursor))
                .collect::<Vec<_>>(),
            vec![(group_topic(b"g2").cloned_vec(), 12)],
            "the re-issue resumes at max(floor, progress)"
        );
        assert_ne!(
            reissue.mutate_id, interrupted.mutate_id,
            "a re-issue is a fresh wave"
        );

        // The re-issued wave's replay completes the lease: only the tail the
        // dead wire never delivered, then the owed CatchUpComplete.
        second.send(replay(
            reissue.mutate_id,
            vec![group_msg(13, b"g2")],
            vec![],
        ));
        second.send(catchup_complete(reissue.mutate_id));
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(13, b"g2")])
            }
            _ => panic!("beta expected the rest of its replay"),
        }
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }

    /// When every leased topic rides a re-issued wave, the reconnect has no
    /// resume wave to carry the `resume()` waiters — they must still resolve
    /// once the re-issued waves complete (a live wire with no wave left in
    /// flight is at least as caught up as the wave they were denied), and
    /// not a moment before.
    #[xmtp_common::test(unwrap_try = true)]
    async fn resume_without_a_resume_wave_settles_when_reissues_complete() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 5)], 8).await?;
        let mut server = take_server(&servers);
        let interrupted = server.next_mutate().await; // never completed

        transport.suspend().await?;
        server.request_stream_ended().await;

        let resumed = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });
        let mut second = wait_for_server(&servers).await;
        let reissue = second.next_mutate().await;
        assert_eq!(reissue.adds[0].id_cursor, 5);
        assert_ne!(reissue.mutate_id, interrupted.mutate_id);

        // Every leased topic is owned by the re-issue, so no resume wave was
        // sent — and the waiter must not resolve while the re-issue is still
        // in flight ("catch up, then done").
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            !resumed.is_finished(),
            "resume() must wait out the re-issued wave"
        );

        second.send(catchup_complete(reissue.mutate_id));
        tokio::time::timeout(WAIT, resumed).await?.unwrap()?;
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }

    /// Live frames served between a lease's registration and the server
    /// processing its Mutate are withheld while the lease is still owed
    /// history — delivered now they would jump ahead of the replay — and
    /// join its feed at the catch-up, after the requested history. The
    /// wave's replay then chases the live edge across the same segment and
    /// must not repeat it (the owner lane dedups at the registration
    /// snapshot; the flush dedups at the owed watermark): the lease sees
    /// its whole suffix in cursor order, exactly once.
    #[xmtp_common::test(unwrap_try = true)]
    async fn replay_does_not_repeat_the_pre_mutate_live_window() {
        let (transport, servers) = transport();
        let topic = group_topic(b"g1");
        // Alpha carries the topic live to cursor 15.
        let mut alpha = transport.lease(vec![(topic.clone(), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(replay(first.mutate_id, vec![group_msg(15, b"g1")], vec![]));
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // Beta re-adds at 10. The live lane races its Mutate: 16 and 17 are
        // served live (legal — the server has not processed the Mutate yet).
        // Beta is still owed (10, 15]; the window is withheld for it.
        let mut beta = transport.lease(vec![(topic.clone(), 10)], 8).await?;
        let wave = server.next_mutate().await;
        server.send(live(
            vec![group_msg(16, b"g1"), group_msg(17, b"g1")],
            vec![],
        ));

        // The wave replays from 10 and crosses over past the live edge: the
        // requested history (11) is beta's alone; the live window (16, 17)
        // must reach beta exactly once — withheld until its catch-up, after
        // the history, never via the replay's re-covering pass.
        server.send(replay(
            wave.mutate_id,
            vec![
                group_msg(11, b"g1"),
                group_msg(16, b"g1"),
                group_msg(17, b"g1"),
            ],
            vec![],
        ));
        server.send(catchup_complete(wave.mutate_id));
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(11, b"g1")], "only the unseen history")
            }
            _ => panic!("beta expected its below-snapshot history"),
        }
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(
                    got,
                    vec![group_msg(16, b"g1"), group_msg(17, b"g1")],
                    "the withheld live window joins the feed at catch-up"
                )
            }
            _ => panic!("beta expected the withheld live window"),
        }
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // Alpha saw the live window once; the replay leaked nothing to it.
        server.send(live(vec![group_msg(18, b"g1")], vec![]));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(16, b"g1"), group_msg(17, b"g1")])
            }
            _ => panic!("alpha expected the live window"),
        }
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(18, b"g1")], "no replay re-delivery")
            }
            _ => panic!("alpha expected only the fresh frame"),
        }
    }

    /// A re-issued wave replays across a live window withheld for its owner
    /// before the wire died: the registration snapshot keeps the re-replay
    /// from delivering it twice, the flush at catch-up delivers it once —
    /// while the history the dead wire never delivered still arrives, in
    /// order, ahead of it.
    #[xmtp_common::test(unwrap_try = true)]
    async fn reissued_replay_skips_frames_the_owner_saw_live() {
        let (transport, servers) = transport();
        let topic = group_topic(b"g1");
        let mut alpha = transport.lease(vec![(topic.clone(), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(replay(first.mutate_id, vec![group_msg(15, b"g1")], vec![]));
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // Beta's wave delivers 11; the live lane races ahead with 16 and 17
        // (withheld — beta is still owed (11, 15]); then the wire dies mid
        // catch-up.
        let mut beta = transport.lease(vec![(topic.clone(), 10)], 8).await?;
        let interrupted = server.next_mutate().await;
        server.send(live(
            vec![group_msg(16, b"g1"), group_msg(17, b"g1")],
            vec![],
        ));
        server.send(replay(
            interrupted.mutate_id,
            vec![group_msg(11, b"g1")],
            vec![],
        ));
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(11, b"g1")]),
            _ => panic!("beta expected its partial replay"),
        }
        drop(server);

        // The topic is owned by the interrupted wave, so the re-issue seeds
        // the open — from the wave's own progress (11), not the live edge.
        let mut second = wait_for_server(&servers).await;
        let reissue = second.next_mutate().await;
        assert_eq!(
            reissue
                .adds
                .iter()
                .map(|add| (add.topic.clone(), add.id_cursor))
                .collect::<Vec<_>>(),
            vec![(topic.cloned_vec(), 11)],
            "the re-issue resumes the replay where it was interrupted"
        );

        // Its replay re-covers the live window on the way to the crossover:
        // the snapshot skips the re-delivery, the flush hands beta the
        // withheld window once — after the history, before its marker.
        second.send(replay(
            reissue.mutate_id,
            vec![
                group_msg(12, b"g1"),
                group_msg(16, b"g1"),
                group_msg(17, b"g1"),
            ],
            vec![],
        ));
        second.send(catchup_complete(reissue.mutate_id));
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(12, b"g1")], "only the unseen history")
            }
            _ => panic!("beta expected the rest of its history"),
        }
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(
                    got,
                    vec![group_msg(16, b"g1"), group_msg(17, b"g1")],
                    "the withheld live window joins the feed at catch-up"
                )
            }
            _ => panic!("beta expected the withheld live window"),
        }
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // The caught-up sibling saw none of the re-issued replay: its next
        // events are the live window it already had, then only fresh frames.
        second.send(live(vec![group_msg(18, b"g1")], vec![]));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(16, b"g1"), group_msg(17, b"g1")])
            }
            _ => panic!("alpha expected the live window"),
        }
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(18, b"g1")], "no replay re-delivery")
            }
            _ => panic!("alpha expected only the fresh frame"),
        }
    }

    /// A reconnect of a shared topic must not resume at an interrupted
    /// wave's high floor when a caught-up sibling sits lower: the sibling's
    /// outage gap exists only below that floor, and a re-add above it would
    /// lose the gap for good. The re-add rides the interrupted wave at the
    /// meet — putting frames below its owner's floor on the wire, which the
    /// per-lease floor guard skips for that owner (its durable store already
    /// has them) while the lower sibling still receives its gap.
    #[xmtp_common::test(unwrap_try = true)]
    async fn shared_topic_reconnect_replays_a_caught_up_siblings_outage_gap() {
        let (transport, servers) = transport();
        let topic = group_topic(b"g1");
        // Alpha goes fully live at cursor 8.
        let mut alpha = transport.lease(vec![(topic.clone(), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(replay(first.mutate_id, vec![group_msg(8, b"g1")], vec![]));
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // Beta asks for the topic from 30 — its wave never completes.
        let mut beta = transport.lease(vec![(topic.clone(), 30)], 8).await?;
        let _interrupted = server.next_mutate().await;
        drop(server);

        // No resume wave (the one topic is owned by the interrupted wave):
        // the re-issue re-adds at the caught-up sibling's position, not at
        // beta's floor.
        let mut second = wait_for_server(&servers).await;
        let reissue = second.next_mutate().await;
        assert_eq!(
            reissue
                .adds
                .iter()
                .map(|add| (add.topic.clone(), add.id_cursor))
                .collect::<Vec<_>>(),
            vec![(topic.cloned_vec(), 8)],
            "the re-add must sit at the caught-up sibling's position"
        );

        // The replay carries alpha's outage gap (9) and beta's history (31).
        // 9 sits below beta's own floor — its durable store already has it,
        // so the floor guard keeps it off beta's feed; 31 reaches both.
        second.send(replay(
            reissue.mutate_id,
            vec![group_msg(9, b"g1"), group_msg(31, b"g1")],
            vec![],
        ));
        second.send(catchup_complete(reissue.mutate_id));
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(
                    got,
                    vec![group_msg(31, b"g1")],
                    "below-floor history must not re-deliver to beta"
                )
            }
            _ => panic!("beta expected its above-floor share of the replay"),
        }
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(
                    got,
                    vec![group_msg(9, b"g1"), group_msg(31, b"g1")],
                    "the caught-up sibling recovers its outage gap"
                )
            }
            _ => panic!("alpha expected its outage gap"),
        }
    }

    /// A wave's per-kind progress is driven by its furthest topic; a
    /// reconnect re-add must clamp it to each topic's own wire position, or
    /// the slower topic's undelivered tail is skipped for good.
    #[xmtp_common::test(unwrap_try = true)]
    async fn reissue_clamps_per_kind_progress_to_each_topics_own_position() {
        let (transport, servers) = transport();
        let mut beta = transport
            .lease(vec![(group_topic(b"g1"), 40), (group_topic(b"g2"), 40)], 8)
            .await?;
        let mut server = take_server(&servers);
        let interrupted = server.next_mutate().await;
        // g2 reaches 50, then g1 drives the per-kind progress to 60.
        server.send(replay(
            interrupted.mutate_id,
            vec![group_msg(50, b"g2")],
            vec![],
        ));
        server.send(replay(
            interrupted.mutate_id,
            vec![group_msg(55, b"g1"), group_msg(60, b"g1")],
            vec![],
        ));
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        drop(server);

        let mut second = wait_for_server(&servers).await;
        let reissue = second.next_mutate().await;
        assert_eq!(
            reissue
                .adds
                .iter()
                .map(|add| (add.topic.clone(), add.id_cursor))
                .collect::<Vec<_>>(),
            vec![
                (group_topic(b"g1").cloned_vec(), 60),
                (group_topic(b"g2").cloned_vec(), 50),
            ],
            "each topic resumes at its own position, not the per-kind progress"
        );

        // The tail the per-kind progress would have skipped still arrives.
        second.send(replay(
            reissue.mutate_id,
            vec![group_msg(52, b"g2"), group_msg(58, b"g2")],
            vec![],
        ));
        second.send(catchup_complete(reissue.mutate_id));
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(52, b"g2"), group_msg(58, b"g2")])
            }
            _ => panic!("beta expected the slower topic's tail"),
        }
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }

    /// A lower re-add moves a topic into the re-adder's wave, and a wave
    /// fully yanked that way resolves immediately (always-ack) — before the
    /// history its lease asked for has been delivered. The lease's
    /// `CatchUpComplete` must be deferred until the claiming wave resolves,
    /// or a fetch-style consumer would deref and lose the tail.
    #[xmtp_common::test(unwrap_try = true)]
    async fn fully_yanked_wave_defers_catch_up_until_the_claiming_wave_resolves() {
        let (transport, servers) = transport();
        let topic = group_topic(b"g1");
        // Gamma carries the topic live to 60 first, so the replays below sit
        // at-or-below the wire position — the owed-history lane, not the
        // fresh fan-out, is what routes them.
        let mut gamma = transport.lease(vec![(topic.clone(), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(replay(first.mutate_id, vec![group_msg(60, b"g1")], vec![]));
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut gamma).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(matches!(
            recv(&mut gamma).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        let mut alpha = transport.lease(vec![(topic.clone(), 50)], 8).await?;
        let yanked = server.next_mutate().await;
        // Beta's lower re-add claims the topic out of alpha's wave...
        let mut beta = transport.lease(vec![(topic.clone(), 20)], 8).await?;
        let claiming = server.next_mutate().await;
        // ...so the server acks alpha's now-empty wave at once, then serves
        // the whole replay — alpha's slice included — inside beta's wave.
        server.send(catchup_complete(yanked.mutate_id));
        server.send(replay(
            claiming.mutate_id,
            vec![group_msg(30, b"g1"), group_msg(55, b"g1")],
            vec![],
        ));
        server.send(catchup_complete(claiming.mutate_id));

        // The events arrive in order, so alpha seeing its slice BEFORE its
        // CatchUpComplete proves the completion was deferred past the ack —
        // and the slice is exactly (floor 50, snapshot 60]: not the 30 its
        // durable store already holds.
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(55, b"g1")], "alpha's owed slice alone")
            }
            _ => panic!("alpha's history must ride the claiming wave"),
        }
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(30, b"g1"), group_msg(55, b"g1")])
            }
            _ => panic!("beta expected its own replay"),
        }
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // Gamma, caught up at the wire position, saw none of the replay.
        server.send(live(vec![group_msg(61, b"g1")], vec![]));
        match recv(&mut gamma).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(61, b"g1")], "no replay re-delivery")
            }
            _ => panic!("gamma expected only the fresh frame"),
        }
    }

    /// The deferral line is the max of the wire position and the lease's own
    /// floor: when the wire has only ever seen history below the floor, a
    /// claiming re-add between the two must still park the yanked lease's
    /// completion until its replay arrives.
    #[xmtp_common::test(unwrap_try = true)]
    async fn yank_defers_even_when_the_wire_position_trails_the_floor() {
        let (transport, servers) = transport();
        let topic = group_topic(b"g1");
        let mut gamma = transport.lease(vec![(topic.clone(), 5)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(replay(first.mutate_id, vec![group_msg(10, b"g1")], vec![]));
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut gamma).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(matches!(
            recv(&mut gamma).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // The wire position is 10. Alpha asks from 50 (its durable cursor is
        // ahead of anything this wire has seen); beta's re-add at 30 claims
        // the topic out of alpha's wave.
        let mut alpha = transport.lease(vec![(topic.clone(), 50)], 8).await?;
        let yanked = server.next_mutate().await;
        let _beta = transport.lease(vec![(topic.clone(), 30)], 8).await?;
        let claiming = server.next_mutate().await;
        server.send(catchup_complete(yanked.mutate_id));
        // Beta's wave replays past both floors to the live edge.
        server.send(replay(
            claiming.mutate_id,
            vec![group_msg(55, b"g1")],
            vec![],
        ));
        server.send(catchup_complete(claiming.mutate_id));

        // Alpha's completion parked on the claiming wave (its claim, 30,
        // sits below alpha's floor): the frame precedes the marker.
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(55, b"g1")])
            }
            _ => panic!("alpha expected the live-edge frame first"),
        }
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }

    /// A caught-up holder that never saw a delivery still anchors a
    /// reconnect re-add at its own floor: its outage gap starts there, and a
    /// re-add at an interrupted sibling's higher floor would skip it for
    /// good.
    #[xmtp_common::test(unwrap_try = true)]
    async fn reconnect_folds_a_caught_up_holders_floor_when_nothing_was_delivered() {
        let (transport, servers) = transport();
        let topic = group_topic(b"g1");
        let mut gamma = transport.lease(vec![(topic.clone(), 5)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        // Nothing above 5 exists yet: an immediate ack, zero deliveries.
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut gamma).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        drop(server); // the wire dies...

        // ...and during the outage alpha (durable cursor 50) subscribes.
        let _alpha = transport.lease(vec![(topic.clone(), 50)], 8).await?;
        let mut second = wait_for_server(&servers).await;
        let reissue = second.next_mutate().await;
        assert_eq!(
            reissue
                .adds
                .iter()
                .map(|add| (add.topic.clone(), add.id_cursor))
                .collect::<Vec<_>>(),
            vec![(topic.cloned_vec(), 5)],
            "the re-add anchors at the caught-up holder's floor"
        );

        // The replay carries gamma's outage gap.
        second.send(replay(
            reissue.mutate_id,
            vec![group_msg(30, b"g1")],
            vec![],
        ));
        match recv(&mut gamma).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(30, b"g1")])
            }
            _ => panic!("gamma expected its outage gap"),
        }
    }

    /// A lease taken while the wire is down must not cold-open a wire of its
    /// own — the reconnect covers the existing topics and the new lease's
    /// adds together, and every pending marker resolves from the waves the
    /// fresh wire opens with.
    #[xmtp_common::test(unwrap_try = true)]
    async fn lease_during_a_dead_wire_rides_the_resume_open() {
        let (transport, servers) = transport();
        let _alpha = transport.lease(vec![(group_topic(b"g1"), 3)], 8).await?;
        let server = take_server(&servers);
        drop(server);

        let mut beta = transport.lease(vec![(group_topic(b"g2"), 7)], 8).await?;
        let mut second = wait_for_server(&servers).await;

        // Both leases were interrupted mid catch-up, so each re-issues as its
        // own wave from its floor (or, had the lease raced the reconnect
        // timer, beta's adds ride a follow-up wave) — but never a second
        // wire.
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
    /// prunes its topic (and its pending wave) from the ledger, so the
    /// reconnect simply never re-adds it — the fresh wire starts without
    /// the topic at all.
    #[xmtp_common::test(unwrap_try = true)]
    async fn deref_during_a_dead_wire_keeps_the_topic_off_the_reconnect() {
        let (transport, servers) = transport();
        let _alpha = transport.lease(vec![(group_topic(b"g1"), 3)], 8).await?;
        let beta = transport.lease(vec![(group_topic(b"g2"), 7)], 8).await?;
        let server = take_server(&servers);
        drop(server); // the wire dies

        drop(beta); // deref lands while there is no wire to send a remove on

        let mut second = wait_for_server(&servers).await;
        let reissue = second.next_mutate().await;
        assert_eq!(
            reissue
                .adds
                .iter()
                .map(|add| (add.topic.clone(), add.id_cursor))
                .collect::<Vec<_>>(),
            vec![(group_topic(b"g1").cloned_vec(), 3)],
            "the dropped lease's topic must not ride the reconnect"
        );
        assert!(
            reissue.removes.is_empty(),
            "nothing to remove on a fresh wire"
        );
    }

    /// A dropped lease's unsent wave is purged from the outbox — it must never
    /// reach the wire late and replay topics nobody asked for. Remove waves
    /// (unowned) and other leases' waves survive, in order, and the purge
    /// reports exactly the forgotten wave ids (the ledger drops only those).
    #[xmtp_common::test(unwrap_try = true)]
    async fn deref_purges_only_the_dropped_leases_unsent_waves() {
        let mut outbox: Outbox<u32> = Outbox::default();
        outbox.push(Some(LeaseId(1)), WaveId(1), 10);
        outbox.push(None, WaveId(2), 20); // a remove wave — never purged
        outbox.push(Some(LeaseId(2)), WaveId(3), 30);
        outbox.push(Some(LeaseId(1)), WaveId(4), 40);
        let purged = outbox.purge(LeaseId(1));
        assert_eq!(purged, vec![WaveId(1), WaveId(4)]);
        let remaining: Vec<u32> = outbox.waves.iter().map(|(.., wave)| *wave).collect();
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

    impl xmtp_common::RetryableError for Refused {
        fn is_retryable(&self) -> bool {
            false
        }
    }

    /// An opener failure surfaces as `TransportError::Open` and registers
    /// nothing — the transport stays usable for a later attempt.
    #[xmtp_common::test(unwrap_try = true)]
    async fn open_failure_surfaces_and_registers_nothing() {
        let transport = BidiTransport::<V3Binding>::new(
            |_initial| async { Err(OpenError::unretryable(Refused)) },
            false,
        );
        let denied = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await;
        assert!(matches!(denied, Err(TransportError::Open(_))));
        // Still alive: the next attempt reaches the opener again (and fails the
        // same way, proving the ledger task survived the failed open).
        let again = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await;
        assert!(matches!(again, Err(TransportError::Open(_))));
    }

    /// An unretryable failure on the *reconnect* path closes the transport
    /// instead of redialing forever: every lease ends (`next()` → `None`),
    /// and the consumers' re-subscribes carry the refusal to their callers.
    #[xmtp_common::test(unwrap_try = true)]
    async fn unretryable_reconnect_closes_every_lease() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let servers: Servers = Arc::default();
        let dials = Arc::new(AtomicUsize::new(0));
        let transport: BidiTransport<V3Binding> = {
            let sink = servers.clone();
            let dials = dials.clone();
            BidiTransport::new(
                move |initial| {
                    let n = dials.fetch_add(1, Ordering::SeqCst);
                    let sink = sink.clone();
                    async move {
                        // The backend accepts the first dial, then refuses the
                        // surface outright — a server dropping bidi support, not
                        // a flap.
                        if n > 0 {
                            return Err(OpenError::new(Refused));
                        }
                        let (api, server) = mock_pair();
                        sink.lock().unwrap().push(server);
                        BidiConnection::open(&api, initial)
                            .await
                            .map_err(OpenError::new)
                    }
                },
                false,
            )
        };

        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        drop(server); // the wire dies; the reconnect dial is refused

        assert!(
            recv(&mut alpha).await.is_none(),
            "an unretryable reconnect must end every lease, not redial forever"
        );
        assert_eq!(dials.load(Ordering::SeqCst), 2, "no dial after the refusal");
        // The ledger is gone: a fresh lease reports the transport closed.
        let denied = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await;
        assert!(matches!(denied, Err(TransportError::Closed)));
    }

    /// A pre-delivery-tags backend echoes no `mutate_id` on `CatchupComplete`
    /// (proto3 default `0`; client waves mint from 1), so the first such
    /// frame is a capability verdict: the transport shuts down — every lease
    /// ends and a later `lease()` sees `Closed`, the same tombstone an
    /// unretryable re-open leaves — rather than half-serving a wire whose
    /// replays the live-order shield would silently drop.
    #[xmtp_common::test(unwrap_try = true)]
    async fn an_untagged_backends_completion_tombstones_the_transport() {
        let (transport, servers) = transport();
        let mut lease = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let _ = server.next_mutate().await;

        server.send(catchup_complete(0));

        assert!(
            recv(&mut lease).await.is_none(),
            "an untagged completion must end the lease, not resolve a wave"
        );
        assert!(
            matches!(
                transport.lease(vec![(group_topic(b"g2"), 0)], 8).await,
                Err(TransportError::Closed)
            ),
            "a later lease must see the tombstone"
        );
    }

    /// The server acks EVERY Mutate — including the uncorrelated
    /// removes-only wave a retire sends — by echoing its `mutate_id`. That
    /// wave must therefore carry a minted id, never `0`: a zero echo reads
    /// as a pre-tags backend and would tombstone a healthy transport (found
    /// by the live fuzz — the scripted suite never acked removes).
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_retires_removes_wave_is_acked_without_tombstoning() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let beta = transport.lease(vec![(group_topic(b"g2"), 0)], 8).await?;
        let mut server = take_server(&servers);
        for _ in 0..2 {
            let wave = server.next_mutate().await;
            server.send(catchup_complete(wave.mutate_id));
        }
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // Beta's deref retires g2: a removes-only wave with a minted id.
        drop(beta);
        let removes = server.next_mutate().await;
        assert!(removes.adds.is_empty());
        assert_eq!(removes.removes, vec![group_topic(b"g2").cloned_vec()]);
        assert_ne!(
            removes.mutate_id, 0,
            "a removes wave must be distinguishable from an untagged echo"
        );

        // The always-ack echo resolves no wave and must not disturb the
        // transport: alpha keeps its live feed and new leases still work.
        server.send(catchup_complete(removes.mutate_id));
        server.send(live(vec![group_msg(7, b"g1")], vec![]));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(7, b"g1")]),
            _ => panic!("alpha must keep receiving after a retire ack"),
        }
        let _gamma = transport
            .lease(vec![(group_topic(b"g3"), 0)], 8)
            .await
            .expect("the transport must survive a retire ack");
    }

    /// A lease dropping mid-replay must not take its in-flight wave's
    /// bookkeeping down with it: the wave still exists on the server, its
    /// replay is still arriving, and it may carry a SURVIVING holder's owed
    /// history. Forgetting it at deref would release the survivor's parked
    /// completion early — caught-up mid-replay, owed frames skipped, silent
    /// loss. The wave must stay pending (owner dangling) and resolve at its
    /// own `CatchUpComplete`.
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_dropped_owners_in_flight_wave_still_serves_the_leases_it_holds() {
        let (transport, servers) = transport();
        let topic = group_topic(b"g1");
        // Gamma carries the topic live to 60, so replays route via the
        // owed-history lane, not the fresh fan-out.
        let mut gamma = transport.lease(vec![(topic.clone(), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(replay(first.mutate_id, vec![group_msg(60, b"g1")], vec![]));
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut gamma).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(matches!(
            recv(&mut gamma).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // Alpha asks from 50; beta's lower re-add at 20 yanks the topic into
        // its wave, and alpha's completion parks on it (fully-yanked ack).
        let mut alpha = transport.lease(vec![(topic.clone(), 50)], 8).await?;
        let yanked = server.next_mutate().await;
        let beta = transport.lease(vec![(topic.clone(), 20)], 8).await?;
        let claiming = server.next_mutate().await;
        server.send(catchup_complete(yanked.mutate_id));

        // Beta drops while its wave is still replaying on the server.
        drop(beta);

        // The wave's replay keeps arriving and must still reach alpha's
        // owed slice — (floor 50, snapshot 60] — before alpha's marker,
        // which only its own `CatchUpComplete` may release.
        server.send(replay(
            claiming.mutate_id,
            vec![group_msg(30, b"g1"), group_msg(55, b"g1")],
            vec![],
        ));
        server.send(catchup_complete(claiming.mutate_id));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(
                    got,
                    vec![group_msg(55, b"g1")],
                    "alpha's owed history must survive its carrier's deref"
                )
            }
            _ => panic!("alpha's owed slice must still arrive after beta drops"),
        }
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }

    /// Deferred completions chain: two successive lower re-adds yank the
    /// topic twice, so the first lease's parked completion re-parks hop by
    /// hop — from the wave that yanked it onto the wave that yanked *that* —
    /// and every lease still receives exactly its owed slice before its
    /// marker. Chains resolve iteratively (each resolved wave's holds are
    /// drained in a loop and either finish or re-park once), so depth costs
    /// one re-check per hop, not stack.
    #[xmtp_common::test(unwrap_try = true)]
    async fn yank_chains_re_park_deferred_completions_hop_by_hop() {
        let (transport, servers) = transport();
        let topic = group_topic(b"g1");
        // Gamma carries the topic live to 60 first, so every replay below
        // routes through the owed-history lane, not the fresh fan-out.
        let mut gamma = transport.lease(vec![(topic.clone(), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(replay(first.mutate_id, vec![group_msg(60, b"g1")], vec![]));
        server.send(catchup_complete(first.mutate_id));
        assert!(matches!(
            recv(&mut gamma).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(matches!(
            recv(&mut gamma).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // Alpha asks from 50; beta's re-add at 40 yanks the topic out of
        // alpha's wave; delta's re-add at 30 yanks it out of beta's.
        let mut alpha = transport.lease(vec![(topic.clone(), 50)], 8).await?;
        let alpha_wave = server.next_mutate().await;
        let mut beta = transport.lease(vec![(topic.clone(), 40)], 8).await?;
        let beta_wave = server.next_mutate().await;
        let mut delta = transport.lease(vec![(topic.clone(), 30)], 8).await?;
        let delta_wave = server.next_mutate().await;

        // The server acks both fully-yanked waves at once (always-ack), then
        // serves the whole replay inside delta's wave.
        server.send(catchup_complete(alpha_wave.mutate_id));
        server.send(catchup_complete(beta_wave.mutate_id));
        server.send(replay(
            delta_wave.mutate_id,
            vec![
                group_msg(35, b"g1"),
                group_msg(45, b"g1"),
                group_msg(55, b"g1"),
            ],
            vec![],
        ));
        server.send(catchup_complete(delta_wave.mutate_id));

        // Per-lease order proves both parked completions survived the hops:
        // each lease sees its owed slice — exactly (its floor, snapshot 60]
        // — BEFORE its marker.
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(55, b"g1")], "alpha's owed slice")
            }
            _ => panic!("alpha's history must survive two yanks"),
        }
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(
                    got,
                    vec![group_msg(45, b"g1"), group_msg(55, b"g1")],
                    "beta's owed slice"
                )
            }
            _ => panic!("beta's history must survive one yank"),
        }
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        match recv(&mut delta).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(
                    got,
                    vec![
                        group_msg(35, b"g1"),
                        group_msg(45, b"g1"),
                        group_msg(55, b"g1"),
                    ],
                    "delta owns the wave and gets the full replay"
                )
            }
            _ => panic!("delta expected its own replay"),
        }
        assert!(matches!(
            recv(&mut delta).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // Gamma, caught up at the wire position, saw none of it.
        server.send(live(vec![group_msg(61, b"g1")], vec![]));
        match recv(&mut gamma).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(61, b"g1")], "no replay re-delivery")
            }
            _ => panic!("gamma expected only the fresh frame"),
        }
    }

    /// Two overlapping waves on a topic the wire has never delivered a
    /// frame for (no `last_seen`, no registration snapshot — a fresh
    /// process catching up). The higher lease's replay races ahead and
    /// advances the wire position; the lower lease must still receive its
    /// FULL asked suffix, in cursor order, exactly once. Regression, found
    /// by the proptest model: a missing snapshot read as "nothing owed",
    /// so the foreign replay's frames reached the lower lease out of order
    /// through the fresh lane and its own replay was then skipped entirely
    /// — the below-overlap frames were silently lost.
    #[xmtp_common::test(unwrap_try = true)]
    async fn overlapping_waves_on_a_virgin_topic_lose_nothing() {
        let (transport, servers) = transport();
        let topic = group_topic(b"g1");
        // History [1..4] exists server-side; the transport has seen none.
        let mut alpha = transport.lease(vec![(topic.clone(), 2)], 8).await?;
        let mut beta = transport.lease(vec![(topic.clone(), 0)], 8).await?;
        let mut server = take_server(&servers);
        let alpha_wave = server.next_mutate().await.mutate_id;
        let beta_wave = server.next_mutate().await.mutate_id;

        // Alpha's wave replays first — (2, 4] — running ahead of the wire
        // position. Beta is not owed these from alpha's wave: its own
        // replay covers them below.
        server.send(replay(
            alpha_wave,
            vec![group_msg(3, b"g1"), group_msg(4, b"g1")],
            vec![],
        ));
        server.send(catchup_complete(alpha_wave));
        // Beta's wave replays its full ask, (0, 4] — a lower add legally
        // overlaps the sibling's range.
        server.send(replay(
            beta_wave,
            vec![group_msg(1, b"g1"), group_msg(2, b"g1")],
            vec![],
        ));
        server.send(replay(
            beta_wave,
            vec![group_msg(3, b"g1"), group_msg(4, b"g1")],
            vec![],
        ));
        server.send(catchup_complete(beta_wave));

        // Alpha: its slice, then its marker.
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(
                    got,
                    vec![group_msg(3, b"g1"), group_msg(4, b"g1")],
                    "alpha's own slice"
                )
            }
            _ => panic!("alpha expected its replay"),
        }
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        // Beta: the complete suffix above its floor, in cursor order,
        // exactly once — however the batches arrive — then its marker.
        let mut got = Vec::new();
        loop {
            match recv(&mut beta).await {
                Some(LeaseEvent::GroupMessages(batch)) => got.extend(batch),
                Some(LeaseEvent::CatchUpComplete) => break,
                _ => panic!("beta expected replay or completion"),
            }
        }
        assert_eq!(
            got,
            vec![
                group_msg(1, b"g1"),
                group_msg(2, b"g1"),
                group_msg(3, b"g1"),
                group_msg(4, b"g1"),
            ],
            "beta owns the full suffix above 0, ordered, exactly once"
        );
    }

    /// A wire death in the middle of a virgin-topic catch-up (no
    /// registration snapshot). The re-issue must resume from the floor
    /// pushed past what replays delivered — never from the wire position,
    /// which may sit past history the lease was never served. Companion
    /// regression to the demux one above, for `reconnect_plan`'s owed
    /// derivation.
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_virgin_topics_interrupted_catch_up_resumes_from_its_floor() {
        let (transport, servers) = transport();
        let topic = group_topic(b"g1");
        let mut alpha = transport.lease(vec![(topic.clone(), 2)], 8).await?;
        let mut beta = transport.lease(vec![(topic.clone(), 0)], 8).await?;
        let mut server = take_server(&servers);
        server.next_mutate().await;
        server.next_mutate().await;

        // Alpha's replay lands and advances the wire position to 4; beta's
        // never starts. Then the wire dies.
        server.send(replay(
            1,
            vec![group_msg(3, b"g1"), group_msg(4, b"g1")],
            vec![],
        ));
        drop(server);

        // The reconnect must re-ask the shared topic at the meet of every
        // catching-up holder's candidate — beta's floor 0, nothing replayed
        // — never at the wire position 4.
        let mut server = wait_for_server(&servers).await;
        let reissue = server.next_mutate().await;
        let cursors: Vec<u64> = reissue
            .adds
            .iter()
            .filter(|add| add.topic == topic.cloned_vec())
            .map(|add| add.id_cursor)
            .collect();
        assert_eq!(
            cursors,
            vec![0],
            "the re-ask must carry beta's floor, not the wire position"
        );

        // The re-served replay reaches beta whole; alpha gets no re-delivery.
        let wave = reissue.mutate_id;
        server.send(replay(
            wave,
            vec![
                group_msg(1, b"g1"),
                group_msg(2, b"g1"),
                group_msg(3, b"g1"),
                group_msg(4, b"g1"),
            ],
            vec![],
        ));
        server.send(catchup_complete(wave));

        let mut got = Vec::new();
        loop {
            match recv(&mut beta).await {
                Some(LeaseEvent::GroupMessages(batch)) => got.extend(batch),
                Some(LeaseEvent::CatchUpComplete) => break,
                _ => panic!("beta expected replay or completion"),
            }
        }
        assert_eq!(
            got,
            vec![
                group_msg(1, b"g1"),
                group_msg(2, b"g1"),
                group_msg(3, b"g1"),
                group_msg(4, b"g1"),
            ],
            "beta's interrupted catch-up must resume below the wire position"
        );
        // Alpha received its pre-death slice exactly once.
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(3, b"g1"), group_msg(4, b"g1")])
            }
            _ => panic!("alpha expected its pre-death replay"),
        }
    }
    /// The withheld-frame buffers are bounded. A wave that never completes
    /// while fresh traffic keeps pouring in (a wedged or misbehaving
    /// server) trips [`WITHHELD_FRAMES_CAP`] and takes the channel-wedge
    /// recovery instead of growing memory without bound: the lease ends,
    /// and its consumer re-leases from durable cursors.
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_withheld_frame_overflow_drops_the_lease_for_recovery() {
        let (transport, servers) = transport();
        let topic = group_topic(b"g1");
        let mut beta = transport.lease(vec![(topic.clone(), 0)], 8).await?;
        let mut server = take_server(&servers);
        server.next_mutate().await; // the wave never completes

        // Live frames arrive while beta is still owed history: withheld,
        // one by one, until the cap trips.
        for id in 1..=(WITHHELD_FRAMES_CAP as u64 + 1) {
            server.send(live(vec![group_msg(id, b"g1")], vec![]));
        }
        assert!(
            recv(&mut beta).await.is_none(),
            "an overflowing lease must be closed for durable-cursor recovery"
        );
    }
    /// The completion flush replays withheld frames in arrival order
    /// within each kind — at most one event per kind, first-arrived kind
    /// leading — never hash-map order across a kind's topics, and never
    /// one event per kind run (which could burst the lease's channel
    /// bound; see the alternating-window test below).
    #[xmtp_common::test(unwrap_try = true)]
    async fn flush_replays_withheld_frames_per_kind_in_arrival_order() {
        let (transport, servers) = transport();
        let (gt, wt) = (group_topic(b"g1"), welcome_topic(b"ik1"));
        // Alpha carries both topics live, seeding the wire positions so
        // beta's owed slices are bounded (snapshots exist).
        let mut alpha = transport
            .lease(vec![(gt.clone(), 0), (wt.clone(), 0)], 8)
            .await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(replay(
            first.mutate_id,
            vec![group_msg(15, b"g1")],
            vec![welcome_msg(5, b"ik1")],
        ));
        server.send(catchup_complete(first.mutate_id));
        for _ in 0..2 {
            assert!(matches!(
                recv(&mut alpha).await,
                Some(LeaseEvent::GroupMessages(_) | LeaseEvent::WelcomeMessages(_))
            ));
        }
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        // Beta asks for both from 0; the live lane races its wave with an
        // interleaved window — group 16, welcome 9, group 17 — withheld in
        // that order.
        let mut beta = transport
            .lease(vec![(gt.clone(), 0), (wt.clone(), 0)], 8)
            .await?;
        let wave = server.next_mutate().await;
        server.send(live(vec![group_msg(16, b"g1")], vec![]));
        server.send(live(vec![], vec![welcome_msg(9, b"ik1")]));
        server.send(live(vec![group_msg(17, b"g1")], vec![]));
        // The wave replays the owed history, then completes.
        server.send(replay(wave.mutate_id, vec![group_msg(11, b"g1")], vec![]));
        server.send(replay(wave.mutate_id, vec![], vec![welcome_msg(3, b"ik1")]));
        server.send(catchup_complete(wave.mutate_id));

        // History first, in wire order...
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(11, b"g1")]),
            _ => panic!("beta expected its group history"),
        }
        match recv(&mut beta).await {
            Some(LeaseEvent::WelcomeMessages(got)) => {
                assert_eq!(got, vec![welcome_msg(3, b"ik1")])
            }
            _ => panic!("beta expected its welcome history"),
        }
        // ...then the withheld window: one event per kind, arrival order
        // within the kind, and the first-arrived kind (group 16 beat
        // welcome 9 to the wire) leading.
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(16, b"g1"), group_msg(17, b"g1")])
            }
            _ => panic!("beta expected the withheld group frames first"),
        }
        match recv(&mut beta).await {
            Some(LeaseEvent::WelcomeMessages(got)) => {
                assert_eq!(got, vec![welcome_msg(9, b"ik1")])
            }
            _ => panic!("beta expected the withheld welcome frame"),
        }
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }

    /// An alternating group/welcome withheld window must flush within the
    /// lease's channel bound — at most one event per kind plus the
    /// completion marker — never one `try_send` per kind run, which would
    /// drop a healthy lease that merely had not drained yet.
    #[xmtp_common::test(unwrap_try = true)]
    async fn an_alternating_withheld_window_flushes_within_the_channel_bound() {
        let (transport, servers) = transport();
        let (gt, wt) = (group_topic(b"g1"), welcome_topic(b"ik1"));
        // Alpha seeds the wire positions so beta's owed slices are bounded.
        let mut alpha = transport
            .lease(vec![(gt.clone(), 0), (wt.clone(), 0)], 8)
            .await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.send(replay(
            first.mutate_id,
            vec![group_msg(15, b"g1")],
            vec![welcome_msg(5, b"ik1")],
        ));
        server.send(catchup_complete(first.mutate_id));
        for _ in 0..3 {
            recv(&mut alpha).await;
        }

        // Beta withholds a 12-frame alternating window — 11 kind
        // transitions, far past its channel depth of 8 if each run became
        // its own event — while its wave is still open.
        let mut beta = transport
            .lease(vec![(gt.clone(), 0), (wt.clone(), 0)], 8)
            .await?;
        let wave = server.next_mutate().await;
        for i in 0..6u64 {
            server.send(live(vec![group_msg(16 + i, b"g1")], vec![]));
            server.send(live(vec![], vec![welcome_msg(6 + i, b"ik1")]));
        }
        server.send(catchup_complete(wave.mutate_id));

        // The flush arrives as exactly one event per kind, then the
        // completion — the lease survives.
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(
                    got,
                    (0..6).map(|i| group_msg(16 + i, b"g1")).collect::<Vec<_>>()
                )
            }
            _ => panic!("beta expected all withheld group frames in one event"),
        }
        match recv(&mut beta).await {
            Some(LeaseEvent::WelcomeMessages(got)) => {
                assert_eq!(
                    got,
                    (0..6)
                        .map(|i| welcome_msg(6 + i, b"ik1"))
                        .collect::<Vec<_>>()
                )
            }
            _ => panic!("beta expected all withheld welcome frames in one event"),
        }
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        // Alive and caught up: fresh traffic flows straight through.
        server.send(live(vec![group_msg(22, b"g1")], vec![]));
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(22, b"g1")]),
            _ => panic!("beta must survive the flush and keep streaming"),
        }
    }
}
