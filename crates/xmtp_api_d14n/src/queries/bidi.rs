//! Backend-agnostic XIP-83 bidirectional subscription *connection* (native-only).
//!
//! This is the control core shared by the v3 and d14n backends: it owns a single
//! bidi stream end-to-end and is the **sole writer** of the request half. It
//! auto-answers server `Ping`s, correlates client liveness probes, multiplexes
//! caller commands onto the wire, and surfaces only real subscription events —
//! keepalive never reaches the consumer. It also polices liveness on its own:
//! a wire with no inbound frames for the silence budget gets one watchdog ping,
//! and if that too goes unanswered the actor tears down — so a half-open link
//! surfaces as end-of-events instead of a stream that hangs forever.
//!
//! Everything backend-specific (wire types, frame construction, and turning an
//! inbound frame into consumer events — including d14n's `OriginatorEnvelope`
//! extraction) lives behind the [`BidiBinding`] trait.
//! The control logic — probe + timeout, the non-blocking select loop, the
//! give-up cap, `finish()`/half-close, and ownership teardown — lives here, once.
//!
//! Owning *both* halves is the point. The actor holds the wire-outbound sender
//! and the inbound stream; when inbound dies it drops both, so the request half
//! tears down with it and any later `mutate`/`probe` fails with `Closed` by
//! channel ownership — never by silently enqueueing into a stream nothing reads.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use futures::StreamExt;
use futures::stream::BoxStream;
use tokio::sync::{mpsc, oneshot};
use xmtp_common::{AbortHandle, MaybeSend, MaybeSync, StreamHandle};
use xmtp_proto::types::{Topic, TopicKind};

/// Wire-outbound depth. The actor is the sole writer; a transport that stops
/// draining backs frames up here first, then in the actor's `pending` queue,
/// until [`MAX_PENDING_FRAMES`] declares the wire wedged and the actor gives up.
pub(crate) const WIRE_BUFFER: usize = 64;
/// Caller→actor command depth.
pub(crate) const COMMAND_BUFFER: usize = 64;
/// Actor→caller event depth; large enough that a brief consumer stall doesn't
/// stall wire reads (and thus pong liveness).
pub(crate) const EVENT_BUFFER: usize = 1024;
/// XIP-83 client req 2 fallback keepalive, used until the server's `Started`
/// frame advertises its own cadence.
pub(crate) const DEFAULT_KEEPALIVE_MS: u32 = 30_000;
/// `N` from XIP-83 client req 2 (recommended 2–3): the *default* probe deadline
/// is this many keepalive intervals — generous enough not to false-positive a
/// slow-but-live link. Latency-sensitive callers (e.g. a notification handler)
/// pass a much smaller bound to [`Connection::probe_within`].
///
/// It is also the actor's total silence budget: after `N - 1` intervals with no
/// inbound frame the actor sends a watchdog ping, and after the full `N` it
/// tears down (see [`watchdog_deadline`]).
pub(crate) const PROBE_TIMEOUT_MULTIPLIER: u32 = 3;
/// Nonce for the actor's own watchdog pings. Client probes mint their nonces
/// via [`next_probe_nonce`], which skips this value, so they can never collide
/// with it — and the watchdog doesn't correlate the pong anyway: *any* inbound
/// frame proves the link and resets the window.
const WATCHDOG_NONCE: u64 = u64::MAX;

/// Mint a client probe nonce: counts up from 1, skipping `0` and the reserved
/// [`WATCHDOG_NONCE`] when the counter wraps. The loop settles within three
/// steps — there are only two reserved values.
fn next_probe_nonce(counter: &AtomicU64) -> u64 {
    loop {
        let nonce = counter.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        if nonce != WATCHDOG_NONCE && nonce != 0 {
            return nonce;
        }
    }
}
/// Hard cap on the outbound backlog. Past this, the wire has been wedged long
/// enough that buffering more is pointless — the transport isn't draining the
/// request half, so the link is effectively dead — and the actor gives up,
/// tearing down so the consumer re-opens from cursors on a fresh stream.
/// Reached only under a sustained stall while commands or auto-pongs keep
/// arriving; tearing down here (rather than parking callers forever) is also
/// what keeps a queued `Finish` from starving behind a wedged wire.
pub(crate) const MAX_PENDING_FRAMES: usize = WIRE_BUFFER * 2;
/// Total time the post-`finish` drain will wait for wire capacity while flushing
/// already-accepted frames. Generous for a transient stall (a draining transport
/// frees a slot in milliseconds) yet bounded, so a wedged transport can't hold
/// the half-close hostage — see [`drain_after_finish`].
const DRAIN_FLUSH_BUDGET: Duration = Duration::from_secs(1);

/// Backend-specific wire vocabulary for a bidi subscription. The control core is
/// generic over this: a binding names the wire request/response types and the
/// per-backend `Mutate` and message types, builds outbound frames, and — via
/// [`BidiBinding::handle`] — classifies an inbound response into an [`Inbound`]
/// instruction (which is where d14n runs its per-envelope extractors).
pub trait BidiBinding: Send + 'static {
    /// Outbound wire frame (client → server).
    type Request: Send + 'static;
    /// Inbound wire frame (server → client).
    type Response: Send + 'static;
    /// The backend's `Mutate` payload (v3: single `id_cursor`; d14n: vector cursor).
    type Mutate: Send;
    /// Consumer-facing group message (v3: raw proto; d14n: unified,
    /// post-extract). `MaybeSync` because the transport ledger shares
    /// withheld frames between holders via `Arc` inside its actor.
    type GroupMessage: MaybeSend + MaybeSync;
    /// Consumer-facing welcome message (v3: raw proto; d14n: unified,
    /// post-extract). `MaybeSync` for the same reason as `GroupMessage`.
    type WelcomeMessage: MaybeSend + MaybeSync;

    /// Wrap a `Mutate` as an outbound request frame.
    fn mutate_frame(mutate: Self::Mutate) -> Self::Request;
    /// A client `Ping` request frame (liveness probe).
    fn ping_frame(nonce: u64) -> Self::Request;
    /// A `Pong` request frame answering a server `Ping`.
    fn pong_frame(nonce: u64) -> Self::Request;

    /// Classify one inbound response into an actor instruction. An associated
    /// fn, like the frame constructors: bindings are stateless by design — any
    /// stateful ordering or cursor tracking belongs to the consumer, not a
    /// single per-process transport (see the d14n binding's module docs).
    fn handle(response: Self::Response) -> Inbound<Self::GroupMessage, Self::WelcomeMessage>;
}

/// What an inbound frame means to the control core, after the binding classifies
/// it. Liveness (`Ping`/`Pong`) is handled by the actor and never surfaced;
/// everything else becomes a consumer [`Event`].
pub enum Inbound<G, W> {
    /// Server ping — the actor auto-pongs this nonce.
    Ping(u64),
    /// Server pong — the actor resolves the matching client probe.
    Pong(u64),
    /// A single consumer event to surface (handshake / markers).
    Emit(Event<G, W>),
    /// A delivery batch — the actor emits `GroupMessages` then
    /// `WelcomeMessages`, each only if non-empty, both carrying the frame's
    /// wave tag. Kept distinct from `Emit` so a frame carrying both kinds
    /// needs no extra allocation.
    Messages {
        group: Vec<G>,
        welcome: Vec<W>,
        mutate_id: u64,
    },
    /// Nothing to do — unknown version or an informational/undecodable frame.
    Skip,
}

/// Events surfaced to the consumer, in wire order. `Ping`/`Pong` never appear —
/// liveness lives entirely inside the actor. Generic over the backend's group
/// and welcome message types.
#[derive(Debug, Clone, PartialEq)]
pub enum Event<G, W> {
    /// First frame on every stream; carries the server's keepalive cadence.
    Started {
        keepalive_interval_ms: u32,
        capabilities: Vec<i32>,
    },
    /// A `Mutate`'s adds are fully caught up; echoes the Mutate's `mutate_id`.
    CatchUpComplete { mutate_id: u64 },
    /// These topics just crossed from catch-up to live.
    TopicsLive { topics: Vec<Topic> },
    /// A group-message delivery batch. `mutate_id` is the catch-up wave that
    /// produced it (`0` = the live stream) — XIP-83 delivery tags.
    GroupMessages { messages: Vec<G>, mutate_id: u64 },
    /// A welcome delivery batch; tagged like [`Event::GroupMessages`].
    WelcomeMessages { messages: Vec<W>, mutate_id: u64 },
}

#[derive(Debug, thiserror::Error)]
pub enum BidiError {
    #[error("the bidi connection is closed; re-open and resume from durable cursors")]
    Closed,
    #[error("liveness probe timed out; treat the link as dead, drop it, and re-open")]
    ProbeTimedOut,
}

/// Outcome of a [`Connection::try_mutate`] that could not be accepted; both
/// variants hand the mutate back to the caller.
#[derive(Debug)]
pub enum TryMutateError<M> {
    /// The command buffer is momentarily full — retry once the actor has
    /// made progress (e.g. after the caller drains more events).
    Full(M),
    /// The connection is finished or its actor is gone; this wave can only be
    /// re-issued on a fresh connection.
    Closed(M),
}

/// Submitted by the handle, performed by the actor (the sole wire writer).
enum Command<B: BidiBinding> {
    Mutate(B::Mutate),
    /// A client liveness probe. The actor sends the `Ping` and fires `ack` when
    /// the matching `Pong` returns; if the actor exits first, `ack` is dropped
    /// and the waiting [`Connection::probe`] resolves to `Closed`.
    Probe {
        nonce: u64,
        ack: oneshot::Sender<()>,
    },
    /// Half-close the request half. The actor drops its wire sender so the
    /// outbound stream ends (the server sees the half-close), then drains inbound
    /// to completion. No further outbound frames are sent after this.
    Finish,
}

/// A handle to one open bidirectional subscription. Writing to the wire is the
/// actor's job; this only submits commands and reads events. Generic over the
/// backend [`BidiBinding`]; the v3 and d14n modules provide concrete aliases.
pub struct Connection<B: BidiBinding> {
    commands: mpsc::Sender<Command<B>>,
    events: mpsc::Receiver<Event<B::GroupMessage, B::WelcomeMessage>>,
    probe_nonce: AtomicU64,
    /// Latched `true` once a `finish` has *delivered* `Command::Finish` to the
    /// actor; monotonic — set once on delivery, never cleared. Lets `mutate`/
    /// `probe` observe `Closed` without racing the actor's teardown of the
    /// command channel: after `finish` returns, `Command::Finish` is still in
    /// flight and the receiver lives until the actor drains it, so a
    /// FIFO-following `mutate` would otherwise be accepted into the buffer and
    /// then silently dropped. Latching only on delivery (not before the send) is
    /// what makes a cancelled `finish` harmless and concurrent `finish` calls
    /// race-free — there is no reset to race.
    finished: AtomicBool,
    /// The server's advertised keepalive cadence (ms), recorded when
    /// [`Self::next`] surfaces `Started`; `0` until then. Drives the default
    /// probe deadline so `probe` can self-bound without the caller re-deriving
    /// it. A probe issued before `Started` has been consumed falls back to the
    /// 30s-derived default, which the probe docs already sanction as safe.
    keepalive_ms: u32,
    actor: Box<dyn AbortHandle>,
}

impl<B: BidiBinding> Connection<B> {
    /// Open the stream: seed `initial` as the first request frame (it names the
    /// initial topic set with per-topic resume cursors; XIP-83 client req 3),
    /// then hand the outbound frame stream to `transport` to obtain the inbound
    /// frame stream, and spawn the actor. The backend modules wrap this with an
    /// ergonomic `open(api, initial)`.
    pub(crate) async fn start<T, Fut, S, E>(initial: B::Mutate, transport: T) -> Result<Self, E>
    where
        T: FnOnce(BoxStream<'static, B::Request>) -> Fut,
        Fut: Future<Output = Result<S, E>>,
        S: futures::Stream<Item = Result<B::Response, E>> + Send + 'static,
        E: std::fmt::Display + Send + 'static,
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
            .try_send(B::mutate_frame(initial))
            .unwrap_or_else(|_| {
                unreachable!("send into a fresh, empty, owned channel cannot fail")
            });

        // Wrap the receiver as a `Stream` without pulling in `tokio-stream`:
        // `poll_recv` is exactly the poll fn a `Stream` needs.
        let outbound = futures::stream::poll_fn(move |cx| wire_out_rx.poll_recv(cx));
        let inbound = transport(Box::pin(outbound)).await?;

        let actor = xmtp_common::spawn(
            None,
            // Box::pin makes the inbound stream `Unpin` for the select loop without
            // requiring the transport's stream type to be `Unpin` itself.
            run_actor::<B, _, _>(Box::pin(inbound), wire_out, commands_rx, event_tx),
        );

        Ok(Self {
            commands: commands_tx,
            events,
            probe_nonce: AtomicU64::new(0),
            finished: AtomicBool::new(false),
            keepalive_ms: 0,
            actor: actor.abort_handle(),
        })
    }

    /// Add/remove subscriptions in place. Awaits a free command slot
    /// (backpressure is right for a state change); returns `Closed` once the
    /// actor has stopped — the command receiver dies with it.
    pub async fn mutate(&self, mutate: B::Mutate) -> Result<(), BidiError> {
        if self.finished.load(Ordering::Acquire) {
            return Err(BidiError::Closed);
        }
        self.commands
            .send(Command::Mutate(mutate))
            .await
            .map_err(|_| BidiError::Closed)
    }

    /// Non-blocking [`Self::mutate`]: accept the wave into the command buffer or
    /// hand it straight back. Exists for callers that must never park on this
    /// handle — a consumer that is also the sole drainer of [`Self::next`] and
    /// awaits `mutate` while events back up can deadlock against the actor
    /// (each side blocked on the channel only the other drains). Such callers
    /// keep their own retry queue and stay free to drain.
    ///
    /// `Full` returns the mutate for the caller to retry; `Closed` returns it
    /// for a post-mortem (re-open and re-subscribe from durable cursors).
    pub fn try_mutate(&self, mutate: B::Mutate) -> Result<(), TryMutateError<B::Mutate>> {
        if self.finished.load(Ordering::Acquire) {
            return Err(TryMutateError::Closed(mutate));
        }
        self.commands
            .try_send(Command::Mutate(mutate))
            .map_err(|e| {
                let (closed, cmd) = match e {
                    mpsc::error::TrySendError::Full(cmd) => (false, cmd),
                    mpsc::error::TrySendError::Closed(cmd) => (true, cmd),
                };
                let Command::Mutate(mutate) = cmd else {
                    unreachable!("try_mutate only submits Command::Mutate")
                };
                if closed {
                    TryMutateError::Closed(mutate)
                } else {
                    TryMutateError::Full(mutate)
                }
            })
    }

    /// Half-close the request half — signal that we are done sending. The
    /// outbound stream ends, so `mutate` and `probe` thereafter return `Closed`;
    /// any live delivery already in flight keeps arriving until the server closes
    /// its side. Opened with a `history_only` Mutate, this is the bounded-sync
    /// trigger (XIP-83): the server finishes the in-flight catch-up wave, emits
    /// its `TopicsLive` / `CatchUpComplete` markers, and closes the stream, so the
    /// consumer drains [`Self::next`] to `None` and stops. Returns `Closed` if the
    /// actor has already stopped.
    ///
    /// Draining to `None` relies on the server actually closing its side. After
    /// `finish` there is no `probe` escape hatch (it returns `Closed`), so a
    /// consumer that doesn't trust the peer to honor the half-close should bound
    /// its post-`finish` [`Self::next`] with a timeout rather than awaiting `None`
    /// indefinitely.
    ///
    /// Not meant to race a concurrent `mutate`/`probe` from another task on the
    /// same handle: ordering between two tasks' channel sends is undefined, so a
    /// `mutate` racing `finish` may still report `Ok` and then be dropped. The
    /// guarantee is for the sequential caller — once `finish` *returns*, a later
    /// `mutate`/`probe` from that caller sees `Closed`.
    pub async fn finish(&self) -> Result<(), BidiError> {
        // Latch closed only *after* the send is delivered. A cancelled `finish`
        // (future dropped mid-send) never delivered `Finish`, so it must not
        // wedge the handle — leaving the latch untouched is exactly right. And
        // because the latch is monotonic (set on delivery, never cleared), two
        // concurrent `finish` calls can't race a reset. The same-caller contract
        // still holds: a FIFO-following `mutate`/`probe` runs after this store.
        self.commands
            .send(Command::Finish)
            .await
            .map_err(|_| BidiError::Closed)?;
        self.finished.store(true, Ordering::Release);
        Ok(())
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
        if self.finished.load(Ordering::Acquire) {
            return Err(BidiError::Closed);
        }
        let nonce = next_probe_nonce(&self.probe_nonce);
        let (ack, ack_rx) = oneshot::channel();
        self.commands
            .send(Command::Probe { nonce, ack })
            .await
            .map_err(|_| BidiError::Closed)?;
        ack_rx.await.map_err(|_| BidiError::Closed)
    }

    pub(crate) fn default_probe_timeout(&self) -> Duration {
        let keepalive = match self.keepalive_ms {
            0 => DEFAULT_KEEPALIVE_MS,
            ms => ms,
        };
        Duration::from_millis(u64::from(keepalive) * u64::from(PROBE_TIMEOUT_MULTIPLIER))
    }

    /// Next event, in wire order. `None` means the connection ended (server
    /// close, network death, watchdog teardown, or reap) — resume from durable
    /// cursors on a fresh connection.
    ///
    /// Only resolves on an event or end-of-stream, but a half-open link cannot
    /// leave it pending forever: the actor's silence watchdog tears the
    /// connection down after [`PROBE_TIMEOUT_MULTIPLIER`] keepalive intervals
    /// with no inbound frame (one watchdog ping in between), and that teardown
    /// surfaces here as `None`. [`Self::probe`] remains for callers that need
    /// an answer *faster* than the watchdog budget — e.g. a notification
    /// handler deciding within seconds whether to reuse the connection.
    pub async fn next(&mut self) -> Option<Event<B::GroupMessage, B::WelcomeMessage>> {
        let event = self.events.recv().await;
        // Record the server's cadence as `Started` passes through, so a probe
        // issued in reaction to it already gets the keepalive-derived deadline.
        if let Some(Event::Started {
            keepalive_interval_ms,
            ..
        }) = &event
        {
            self.keepalive_ms = *keepalive_interval_ms;
        }
        event
    }
}

impl<B: BidiBinding> Drop for Connection<B> {
    fn drop(&mut self) {
        // Abort the actor so it cannot keep auto-ponging — a zombie keepalive
        // would hold the server-side subscription open forever. The abort drops
        // the actor's wire-outbound and inbound, cancelling the underlying
        // request and tearing the stream down in both directions.
        self.actor.end();
    }
}

/// When the silence watchdog next fires: after `N - 1` keepalive intervals
/// without an inbound frame it probes, and a probed connection gets the one
/// remaining interval — the full `N ×` budget of [`PROBE_TIMEOUT_MULTIPLIER`] —
/// before teardown. Recomputed every `select!` iteration, so any inbound frame
/// pushes both deadlines out.
fn watchdog_deadline(
    last_inbound: tokio::time::Instant,
    keepalive: Duration,
    probed: bool,
) -> tokio::time::Instant {
    let intervals = if probed {
        PROBE_TIMEOUT_MULTIPLIER
    } else {
        PROBE_TIMEOUT_MULTIPLIER - 1
    };
    last_inbound + keepalive * intervals
}

/// Hand `frame` to the wire if it has room right now, else queue it for the
/// reserve branch to flush. Returns `false` when the actor should give up and tear
/// down: either the wire is already closed (this frame can't be delivered), or the
/// backlog has grown past [`MAX_PENDING_FRAMES`] — a wedged wire we won't buffer
/// behind forever.
///
/// Reaching the queue means the transport isn't draining the request half —
/// which on a healthy link should essentially never happen — so we warn the
/// first time we fall back to it (not on every frame: an existing backlog skips
/// straight to the queue without re-probing the wire).
#[must_use]
fn enqueue<R>(wire_out: &mpsc::Sender<R>, pending: &mut VecDeque<R>, frame: R) -> bool {
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
            // Transport gone. Signal teardown now rather than queue an undeliverable
            // frame and lean on the reserve branch to notice on a later `select!`
            // iteration — which could accept a few more doomed frames first, since
            // `select!` picks ready branches at random. The frame is dropped; a wire
            // this dead will never send it, and the consumer re-syncs from durable
            // cursors on re-open.
            Err(mpsc::error::TrySendError::Closed(_frame)) => return false,
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
/// reap-then-resume — distinct from outbound/wire pressure, which the queue
/// handles.) It ends when the wire ends/errors or the handle goes away; ending
/// drops `wire_out` (the request half closes, tearing the stream down), the
/// `commands` receiver (so `mutate`/`probe` see `Closed`), the `events` sender
/// (so `next` ends), and any outstanding probe acks (so pending `probe`s see
/// `Closed`). One place, every teardown. It also gives up the same way if the
/// outbound backlog blows past [`MAX_PENDING_FRAMES`] — a wedged wire that will
/// never drain, so we tear down and let the consumer re-open — or if the wire
/// goes silent past the watchdog budget (see [`watchdog_deadline`]): a
/// conformant server keeps pinging, so sustained silence, with one watchdog
/// ping of grace, means a half-open link that will never speak again.
async fn run_actor<B, S, E>(
    mut inbound: S,
    wire_out: mpsc::Sender<B::Request>,
    mut commands: mpsc::Receiver<Command<B>>,
    events: mpsc::Sender<Event<B::GroupMessage, B::WelcomeMessage>>,
) where
    B: BidiBinding,
    S: futures::Stream<Item = Result<B::Response, E>> + Unpin,
    E: std::fmt::Display,
{
    // Outstanding client probes awaiting their `Pong`, keyed by nonce.
    let mut probes: HashMap<u64, oneshot::Sender<()>> = HashMap::new();
    // Outbound frames awaiting wire capacity. We drain these through a `reserve()`
    // branch below rather than `send().await` in a branch body — an `.await` in a
    // `select!` arm blocks the whole loop, so a backed-up wire would stall inbound
    // reads and delay auto-pongs (getting us reaped on an otherwise healthy link).
    let mut pending: VecDeque<B::Request> = VecDeque::new();
    // Set by `Command::Finish`: leave the main loop and drain inbound to close,
    // rather than tear everything down. Distinguishes a deliberate half-close
    // from the wire/handle-gone breaks, which fall straight through to teardown.
    let mut finished = false;
    // Silence-watchdog state. A conformant server pings at `keepalive` cadence,
    // so a healthy wire always shows *some* inbound frame well inside the
    // budget; sustained silence is a half-open link. The cadence starts at the
    // XIP-83 fallback and updates when `Started` advertises the real one.
    let mut keepalive = Duration::from_millis(DEFAULT_KEEPALIVE_MS.into());
    let mut last_inbound = tokio::time::Instant::now();
    let mut watchdog_probed = false;

    loop {
        tokio::select! {
            // Polled in order, so teardown is truly last-resort: the watchdog
            // arm at the bottom is only reached when no flush, command, or
            // inbound frame is ready — a pong landing in the same poll as the
            // deadline always wins the tie and restamps the window. (A command
            // flood can starve the deadline check, but a silent wire under a
            // command flood hits `enqueue`'s give-up cap and tears down there.)
            biased;
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
            // A command from the handle: mutate, probe, or finish. Never gated:
            // a backlog-room gate would starve a queued `Finish` forever on a
            // wedged wire (the give-up cap is only checked when a frame is
            // enqueued, and a closed gate means nothing is). Backlog growth from
            // accepted mutates/probes is bounded by `enqueue`'s
            // [`MAX_PENDING_FRAMES`] give-up instead — a wedged wire tears down
            // rather than parking callers forever.
            cmd = commands.recv() => {
                let Some(cmd) = cmd else {
                    // Every handle dropped — nothing more will be sent.
                    break;
                };
                let queued = match cmd {
                    Command::Mutate(mutate) => {
                        enqueue(&wire_out, &mut pending, B::mutate_frame(mutate))
                    }
                    Command::Probe { nonce, ack } => {
                        // Sweep entries whose probe already timed out client-side
                        // (their receiver is gone), so a scheduled prober against
                        // a pong-less peer can't grow the map without bound.
                        probes.retain(|_, ack| !ack.is_closed());
                        probes.insert(nonce, ack);
                        enqueue(&wire_out, &mut pending, B::ping_frame(nonce))
                    }
                    Command::Finish => {
                        // Half-close: stop accepting new commands, then flush the
                        // backlog and drain inbound to close (in `drain_after_finish`).
                        finished = true;
                        break;
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
                match B::handle(response) {
                    // Liveness is internal: auto-pong / probe-correlate here, never
                    // surface it to the consumer.
                    Inbound::Ping(nonce) => {
                        // Auto-pong: hand it to the wire (or queue it FIFO behind a
                        // backlog). A busy wire delays but never drops the pong, and
                        // never blocks us from reading the next frame — but if the
                        // backlog has blown past the cap, the wire is wedged and we
                        // give up rather than keep piling pongs into a dead stream.
                        if !enqueue(&wire_out, &mut pending, B::pong_frame(nonce)) {
                            break;
                        }
                    }
                    Inbound::Pong(nonce) => {
                        // Resolve the matching client probe. Unmatched pongs are
                        // ignored — never surfaced to the consumer.
                        if let Some(ack) = probes.remove(&nonce) {
                            let _ = ack.send(());
                        }
                    }
                    // Consumer-facing frames. The same emit path feeds the live loop
                    // and the post-`finish` drain, so their event semantics match.
                    instruction => {
                        // Adopt the server's advertised cadence for the watchdog as
                        // `Started` passes through (0 keeps the fallback).
                        if let Inbound::Emit(Event::Started {
                            keepalive_interval_ms,
                            ..
                        }) = &instruction
                            && *keepalive_interval_ms > 0
                        {
                            keepalive = Duration::from_millis((*keepalive_interval_ms).into());
                        }
                        if emit_instruction(&events, instruction).await {
                            break;
                        }
                    }
                }
                // Stamp *after* the frame is fully handled, not on receipt: the
                // emit above can park on consumer backpressure for a long time,
                // and that stall is the consumer's, not the wire's — inbound
                // frames may be sitting unread behind it. Restarting the window
                // when we resume listening keeps a slow consumer from reading
                // as wire silence and tearing down a healthy link.
                last_inbound = tokio::time::Instant::now();
                watchdog_probed = false;
            }
            // The silence watchdog. Fires only while the arms above are idle
            // (enforced by `biased`) — exactly the "nothing inbound" condition
            // it exists to detect. One
            // unanswered ping stands between silence and teardown, so a quiet
            // but live link (a server between keepalives) is never reaped: any
            // inbound frame — the pong included — resets the window above.
            _ = tokio::time::sleep_until(watchdog_deadline(last_inbound, keepalive, watchdog_probed)) => {
                if watchdog_probed {
                    tracing::warn!(
                        silent_for_ms = last_inbound.elapsed().as_millis() as u64,
                        "bidi wire silent past the watchdog budget (probe unanswered); \
                         tearing down so the consumer re-opens from cursors"
                    );
                    break;
                }
                watchdog_probed = true;
                if !enqueue(&wire_out, &mut pending, B::ping_frame(WATCHDOG_NONCE)) {
                    break;
                }
            }
        }
    }
    // Resolve any in-flight probes now: dropping the probe-ack senders makes each
    // waiting `probe()` see `Closed`. Must happen *before* `drain_after_finish`,
    // which can block on `inbound` for a while on a half-open link — a probe must
    // never hang on a connection that has already left the live loop.
    drop(probes);
    // A deliberate half-close drains inbound to close rather than tearing down;
    // every other exit reason falls straight through to teardown.
    if finished {
        drain_after_finish::<B, _, _>(inbound, wire_out, pending, commands, &events).await;
    }
    // One exit log for every reason (wire ended/errored, handle gone, or finished
    // draining). The error a caller sees is just `Closed`; the "why" lives here.
    tracing::debug!("bidi subscription actor stopped");
    // `wire_out`, `commands`, `events`, and the `pending` queue drop here — that
    // *is* the teardown in both directions.
}

/// Surface the consumer events for one classified inbound frame, returning true
/// if the consumer is gone (the actor should stop). `Ping`/`Pong` must already
/// have been handled by the caller — passing them here is a no-op. Shared by the
/// live loop and the post-`finish` drain so both deliver identical events.
async fn emit_instruction<G, W>(
    events: &mpsc::Sender<Event<G, W>>,
    instruction: Inbound<G, W>,
) -> bool {
    match instruction {
        Inbound::Emit(event) => emit(events, event).await,
        Inbound::Messages {
            group,
            welcome,
            mutate_id,
        } => {
            if !group.is_empty()
                && emit(
                    events,
                    Event::GroupMessages {
                        messages: group,
                        mutate_id,
                    },
                )
                .await
            {
                return true;
            }
            if !welcome.is_empty()
                && emit(
                    events,
                    Event::WelcomeMessages {
                        messages: welcome,
                        mutate_id,
                    },
                )
                .await
            {
                return true;
            }
            false
        }
        // Ping/Pong are the caller's job; Skip is a no-op.
        Inbound::Ping(_) | Inbound::Pong(_) | Inbound::Skip => false,
    }
}

/// Drain inbound to completion after a half-close. First flush any frames the
/// caller already had accepted (a `mutate` it got `Ok` for, an auto-pong, a
/// probe ping queued under backpressure) so half-close doesn't silently discard
/// them. Then dropping `wire_out` ends the outbound stream (the server sees the
/// half-close, finishes the wave, and closes its side); dropping `commands`
/// makes any late `mutate`/`probe` see `Closed`. We keep surfacing events until
/// the server closes inbound (`next` -> `None`) or the consumer goes away.
/// Liveness frames can't be answered (the wire is gone), so they are ignored;
/// outstanding probes drop with the actor and resolve to `Closed`.
async fn drain_after_finish<B, S, E>(
    mut inbound: S,
    wire_out: mpsc::Sender<B::Request>,
    mut pending: VecDeque<B::Request>,
    commands: mpsc::Receiver<Command<B>>,
    events: &mpsc::Sender<Event<B::GroupMessage, B::WelcomeMessage>>,
) where
    B: BidiBinding,
    S: futures::Stream<Item = Result<B::Response, E>> + Unpin,
    E: std::fmt::Display,
{
    // Flush the accepted-but-unsent backlog before closing the request half,
    // waiting for wire capacity under one shared budget. Bounded on purpose: an
    // unbounded `reserve().await` on a stalled transport (wire full, server no
    // longer reading the request stream) would block forever, so `drop(wire_out)`
    // would never run and the half-close would never reach the server — but a
    // purely non-blocking flush drops accepted frames on a *transient* stall a
    // healthy transport clears in milliseconds. Frames still unplaced when the
    // budget runs out are lost; the consumer re-syncs from durable cursors on
    // re-open, and a wedged actor is the strictly worse outcome.
    let flush_deadline = tokio::time::Instant::now() + DRAIN_FLUSH_BUDGET;
    while let Some(frame) = pending.pop_front() {
        match tokio::time::timeout_at(flush_deadline, wire_out.reserve()).await {
            Ok(Ok(permit)) => permit.send(frame),
            Ok(Err(_)) => break, // wire gone — close rather than block
            Err(_elapsed) => {
                tracing::warn!(
                    dropped = pending.len() + 1,
                    "bidi half-close flush budget exhausted on a stalled wire; \
                     dropping the remaining backlog (re-open re-syncs from cursors)"
                );
                break;
            }
        }
    }
    drop(wire_out);
    drop(commands);
    while let Some(frame) = inbound.next().await {
        let response = match frame {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("bidi stream errored during finish drain: {e}");
                break;
            }
        };
        if emit_instruction(events, B::handle(response)).await {
            break; // consumer gone
        }
    }
}

/// Send one event to the consumer, awaiting a free slot (the backpressure that
/// stalls wire reads once [`EVENT_BUFFER`] fills). Returns true when the
/// consumer is gone and the actor should shut down. (The server's keepalive
/// cadence is recorded handle-side, when [`Connection::next`] surfaces
/// `Started` — every event reaches the consumer through there.)
async fn emit<G, W>(events: &mpsc::Sender<Event<G, W>>, event: Event<G, W>) -> bool {
    events.send(event).await.is_err()
}

/// Parse kind-prefixed wire bytes into typed `Topic`s, validating each kind byte.
/// A malformed topic should never reach us — it means a server or wire-format bug
/// — so log the offending bytes (a topic is at most 33 bytes: a 1-byte kind + a
/// 32-byte id) and skip it rather than kill the connection. Shared by both
/// backends' `TopicsLive` handling.
pub(crate) fn parse_topics(topics: Vec<Vec<u8>>) -> Vec<Topic> {
    topics
        .into_iter()
        .filter_map(|bytes| {
            // Validate the kind byte against a borrow first, so the hex preview —
            // the only allocation — is built solely on the malformed path; the
            // common all-valid case does no extra work.
            match bytes.first().map(|&b| TopicKind::try_from(b)) {
                Some(Ok(_)) => Topic::try_from(bytes).ok(),
                outcome => {
                    let preview = hex::encode(&bytes[..bytes.len().min(33)]);
                    let reason = match outcome {
                        Some(Err(e)) => e.to_string(),
                        _ => "empty topic".to_string(),
                    };
                    tracing::warn!(
                        topic = %preview,
                        "skipping malformed TopicsLive topic (server/wire-format bug): {reason}"
                    );
                    None
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trivial binding: wire frames are bare `u64`s and nothing is ever
    /// surfaced. Enough to exercise the backend-agnostic control logic directly.
    struct TestBinding;

    impl BidiBinding for TestBinding {
        type Request = u64;
        type Response = u64;
        type Mutate = u64;
        type GroupMessage = ();
        type WelcomeMessage = ();

        fn mutate_frame(mutate: u64) -> u64 {
            mutate
        }
        fn ping_frame(nonce: u64) -> u64 {
            nonce
        }
        fn pong_frame(nonce: u64) -> u64 {
            nonce
        }
        fn handle(_response: u64) -> Inbound<(), ()> {
            Inbound::Skip
        }
    }

    /// Probe nonces skip the reserved values across the wrap: the counter
    /// steps over [`WATCHDOG_NONCE`] and `0`, then resumes counting from 1.
    #[xmtp_common::test]
    fn probe_nonces_never_mint_the_watchdog_nonce() {
        let counter = AtomicU64::new(u64::MAX - 2);
        let minted: Vec<u64> = (0..4).map(|_| next_probe_nonce(&counter)).collect();
        assert_eq!(minted, vec![u64::MAX - 1, 1, 2, 3]);
    }

    /// `finish` is a half-close, not an abort: frames the caller already had
    /// accepted under wire backpressure (parked in the actor's `pending` queue,
    /// not yet on the wire) must be flushed before the request half closes. Drive
    /// `drain_after_finish` directly with a pre-seeded `pending` and an
    /// already-closed inbound, so the flush is the only behaviour under test.
    #[xmtp_common::test(unwrap_try = true)]
    async fn drain_after_finish_flushes_pending_before_closing() {
        let (wire_out, mut wire_rx) = mpsc::channel::<u64>(WIRE_BUFFER);
        let (_commands_tx, commands_rx) = mpsc::channel::<Command<TestBinding>>(COMMAND_BUFFER);
        let (events, _events_rx) = mpsc::channel::<Event<(), ()>>(EVENT_BUFFER);

        // A distinctively-tagged frame queued as if wire backpressure had parked
        // it before the caller half-closed.
        let mut pending = VecDeque::new();
        pending.push_back(7_777_u64);

        // Inbound is already closed, so the drain has nothing to read and returns
        // as soon as the pending flush completes.
        let inbound = futures::stream::empty::<Result<u64, BidiError>>();
        drain_after_finish::<TestBinding, _, _>(inbound, wire_out, pending, commands_rx, &events)
            .await;

        // The queued frame reached the wire instead of being dropped on close,
        // and the wire then closes (the actor dropped its sender).
        assert_eq!(
            wire_rx.recv().await,
            Some(7_777),
            "the pending frame must be flushed to the wire"
        );
        assert_eq!(
            wire_rx.recv().await,
            None,
            "the wire closes once the flush is done"
        );
    }

    /// A *wedged* wire must not hold the half-close hostage: the flush waits out
    /// [`DRAIN_FLUSH_BUDGET`] for capacity, then drops the backlog and still
    /// closes the request half so the server sees the half-close.
    #[xmtp_common::test(unwrap_try = true)]
    async fn drain_after_finish_bounds_the_flush_on_a_wedged_wire() {
        // Capacity 1, pre-filled, receiver never polled: reserve() can never
        // succeed, so only the budget can end the flush.
        let (wire_out, mut wire_rx) = mpsc::channel::<u64>(1);
        let (_commands_tx, commands_rx) = mpsc::channel::<Command<TestBinding>>(COMMAND_BUFFER);
        let (events, _events_rx) = mpsc::channel::<Event<(), ()>>(EVENT_BUFFER);
        wire_out.try_send(1_u64).unwrap();

        let mut pending = VecDeque::new();
        pending.push_back(7_777_u64);

        let inbound = futures::stream::empty::<Result<u64, BidiError>>();
        drain_after_finish::<TestBinding, _, _>(inbound, wire_out, pending, commands_rx, &events)
            .await;

        // The pre-wedged frame is still there; the un-flushable backlog was
        // dropped; and crucially the request half closed anyway.
        assert_eq!(wire_rx.recv().await, Some(1));
        assert_eq!(
            wire_rx.recv().await,
            None,
            "the request half must close even when the flush budget expires"
        );
    }

    /// Binding for the watchdog tests. A frame under `100_000` is a `Started`
    /// advertising that many milliseconds of keepalive — so each test picks a
    /// cadence fast enough to run in real time. [`MSG_FRAME`] delivers one
    /// group message (to park the actor on consumer backpressure); anything
    /// else is a no-op frame that still counts as inbound activity.
    struct WatchdogBinding;
    const MSG_FRAME: u64 = 1_000_000;
    const ACTIVITY_FRAME: u64 = 2_000_000;
    /// Generous ceiling for CI scheduling noise; every wait in these tests
    /// resolves in well under a second on a healthy run.
    const WAIT: Duration = Duration::from_secs(5);

    impl BidiBinding for WatchdogBinding {
        type Request = u64;
        type Response = u64;
        type Mutate = u64;
        type GroupMessage = ();
        type WelcomeMessage = ();

        fn mutate_frame(mutate: u64) -> u64 {
            mutate
        }
        fn ping_frame(nonce: u64) -> u64 {
            nonce
        }
        fn pong_frame(nonce: u64) -> u64 {
            nonce
        }
        fn handle(response: u64) -> Inbound<(), ()> {
            match response {
                ms if ms < 100_000 => Inbound::Emit(Event::Started {
                    keepalive_interval_ms: ms as u32,
                    capabilities: vec![],
                }),
                MSG_FRAME => Inbound::Messages {
                    group: vec![()],
                    welcome: vec![],
                    mutate_id: 0,
                },
                _ => Inbound::Skip,
            }
        }
    }

    struct ActorHarness {
        inbound: mpsc::Sender<Result<u64, BidiError>>,
        wire: mpsc::Receiver<u64>,
        events: mpsc::Receiver<Event<(), ()>>,
        _commands: mpsc::Sender<Command<WatchdogBinding>>,
    }

    /// Spawn `run_actor` over harness-controlled channels, mirroring
    /// [`Connection::start`]'s wiring (`poll_fn` stream + `Box::pin`).
    fn spawn_actor() -> ActorHarness {
        let (inbound_tx, mut inbound_rx) = mpsc::channel::<Result<u64, BidiError>>(2048);
        let (wire_out, wire_rx) = mpsc::channel::<u64>(WIRE_BUFFER);
        let (commands_tx, commands_rx) = mpsc::channel::<Command<WatchdogBinding>>(COMMAND_BUFFER);
        let (events_tx, events_rx) = mpsc::channel::<Event<(), ()>>(EVENT_BUFFER);
        let inbound = futures::stream::poll_fn(move |cx| inbound_rx.poll_recv(cx));
        xmtp_common::spawn(
            None,
            run_actor::<WatchdogBinding, _, _>(Box::pin(inbound), wire_out, commands_rx, events_tx),
        );
        ActorHarness {
            inbound: inbound_tx,
            wire: wire_rx,
            events: events_rx,
            _commands: commands_tx,
        }
    }

    /// Total silence earns exactly one watchdog ping, then teardown: the
    /// half-open link surfaces as end-of-events instead of hanging forever.
    #[xmtp_common::test(unwrap_try = true)]
    async fn watchdog_probes_then_tears_down_a_silent_wire() {
        let mut actor = spawn_actor();
        actor.inbound.send(Ok(150)).await?; // Started: 150ms keepalive
        assert!(matches!(
            tokio::time::timeout(WAIT, actor.events.recv()).await?,
            Some(Event::Started { .. })
        ));

        let ping = tokio::time::timeout(WAIT, actor.wire.recv()).await?;
        assert_eq!(
            ping,
            Some(WATCHDOG_NONCE),
            "the watchdog must probe before giving up"
        );
        assert_eq!(
            tokio::time::timeout(WAIT, actor.events.recv()).await?,
            None,
            "an unanswered watchdog ping must tear the connection down"
        );
    }

    /// A quiet-but-live link is never reaped: any inbound frame — a pong or
    /// otherwise — answers the watchdog ping and resets the window.
    #[xmtp_common::test(unwrap_try = true)]
    async fn an_answered_watchdog_probe_keeps_the_wire_alive() {
        let mut actor = spawn_actor();
        actor.inbound.send(Ok(150)).await?;
        tokio::time::timeout(WAIT, actor.events.recv()).await?;

        // Ride out three whole silence windows, answering each probe.
        for _ in 0..3 {
            let ping = tokio::time::timeout(WAIT, actor.wire.recv()).await?;
            assert_eq!(ping, Some(WATCHDOG_NONCE));
            actor.inbound.send(Ok(ACTIVITY_FRAME)).await?;
        }
        assert!(
            matches!(
                actor.events.try_recv(),
                Err(mpsc::error::TryRecvError::Empty)
            ),
            "an answered probe must keep the connection alive"
        );

        // Stop answering: the next unanswered ping tears it down.
        assert_eq!(tokio::time::timeout(WAIT, actor.events.recv()).await?, None);
    }

    /// A steady trickle of frames well inside the probe threshold never
    /// triggers a probe at all.
    #[xmtp_common::test(unwrap_try = true)]
    async fn inbound_activity_resets_the_watchdog() {
        let mut actor = spawn_actor();
        actor.inbound.send(Ok(200)).await?; // probe would fire at 400ms silent
        tokio::time::timeout(WAIT, actor.events.recv()).await?;

        for _ in 0..20 {
            actor.inbound.send(Ok(ACTIVITY_FRAME)).await?;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            matches!(actor.wire.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
            "no watchdog ping may fire while frames keep arriving"
        );
        assert!(matches!(
            actor.events.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
    }

    /// Consumer backpressure must not read as wire silence: while the actor is
    /// parked emitting into a full event channel, inbound frames sit unread
    /// behind it — the stall is the consumer's, not the wire's. The silence
    /// window restarts when the actor resumes listening, so a slow consumer
    /// never gets a healthy link torn down.
    #[xmtp_common::test(unwrap_try = true)]
    async fn consumer_backpressure_is_not_wire_silence() {
        let mut actor = spawn_actor();
        actor.inbound.send(Ok(200)).await?; // probe at 400ms, teardown at 600ms
        tokio::time::timeout(WAIT, actor.events.recv()).await?;

        // Fill the event buffer plus one: the actor parks mid-emit.
        for _ in 0..(EVENT_BUFFER + 1) {
            actor.inbound.send(Ok(MSG_FRAME)).await?;
        }
        // Hold the park well past the full teardown budget, draining nothing.
        tokio::time::sleep(Duration::from_millis(800)).await;

        // Drain; the actor unparks with a fresh silence window. If the park
        // had counted as silence, both watchdog deadlines are long past and
        // teardown would be immediate.
        let mut delivered = 0;
        while delivered < EVENT_BUFFER + 1 {
            match tokio::time::timeout(WAIT, actor.events.recv()).await? {
                Some(Event::GroupMessages { .. }) => delivered += 1,
                other => panic!("unexpected event while draining: {other:?}"),
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(
            matches!(
                actor.events.try_recv(),
                Err(mpsc::error::TryRecvError::Empty)
            ),
            "a consumer stall must not be read as wire silence"
        );
    }
}
