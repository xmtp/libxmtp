//! Callback adapters over the XIP-83 [`StreamRouter`] — the pull→push bridge
//! the bindings call, plus the pieces that decide *whether* and *where* the
//! bidi path runs.
//!
//! ## One wire per destination
//!
//! Clients dialing the same backend URL share ONE [`BidiTransport`]: topics
//! from all of them multiplex onto a single bidi stream, and the transport's
//! leases keep them apart (welcome topics are per-installation; a group
//! topic two clients share is subscribed once and fanned out). One client
//! per process — mobile — makes this invisible; a process running many
//! clients (agents) gets one wire per backend instead of one per client.
//! Wires are keyed by the URL the client's api client dials
//! ([`XmtpMlsBidiStreams::host`]) — connection identity, not network
//! identity: two clients behind the same proxy share that proxy's wire,
//! while a proxied and a direct client to the same backend keep separate
//! wires and separate failure domains (a severed proxy reconnects alone —
//! the toxiproxy test clients depend on exactly this). The first client to
//! stream to a destination donates its api client to that destination's
//! opener. Auth is not at stake — a shared wire is receive-only (publishes
//! keep each client's own api client and whatever authorization it carries),
//! and on v3 the donated client attaches only version-attribution headers.
//!
//! ## The gate and the latch
//!
//! The bidi path is opt-in via [`BIDI_STREAMS_ENABLED_ENV`], read once at
//! the first stream call: mobile apps set it at process init, agents in
//! their deploy env. Unset (or anything but a truthy value) keeps the legacy
//! streams. The gate is only the opt-in — whether a backend can serve the
//! surface is discovered at its first wire open. A backend that cannot
//! refuses it — gRPC `UNIMPLEMENTED` from a v3 node without the surface, an
//! in-process refusal from the d14n and migration api clients — and once a
//! destination's transport has tombstoned itself on such a refusal, every
//! later stream to it sees `Closed` ([`is_bidi_unsupported`] classifies all
//! three). Any of them latches that destination onto the legacy streams:
//! the stream that hit it switches to the legacy path in place (the first
//! caller loses nothing — nor does a startup burst, where every pre-latch
//! stream hits its own refusal and falls back the same way), and every
//! later dispatch to that destination goes straight to legacy
//! ([`bidi_streams_active`]). Destinations latch independently — a process
//! mixing a bidi-capable v3 backend with, say, a d14n client keeps bidi on
//! the v3 wire while the d14n client's streams ride legacy. Latches reset
//! only with the process, so the env var is safe to enable against any
//! backend mix — the worst case per destination is one refused open and its
//! streams on legacy.
//!
//! Dispatch lives here, not with the bindings: each binding site makes one
//! `*_with_callback_dispatch` call, and the `*_with_callback_bidi` entry
//! points are the bidi arm.
//!
//! ## Pump semantics
//!
//! Each stream is one spawned task: subscribe (ready fires once the lease is
//! registered), then drain the router stream into the callback. A subscribe
//! failure warns, fires `on_close()`, and carries the error out through the
//! handle's result — legacy watchdog semantics; the warn is the reliable
//! trace, since the bindings' closers rarely read the result. The exception
//! is a latch-worthy refusal or dead-end open (above), which hands the
//! stream's legacy fallback factory to the same watchdog runner the legacy
//! entry points spawn — stale-trip re-subscribe and reconnect throttle
//! included — instead of closing. `on_close` fires once
//! at natural end — the wire-facing lease survives reconnects and suspends
//! (transport docs), so a natural end means this consumer fell behind and
//! was dropped, or the client shut down; re-subscribing recovers from
//! durable cursors. An `end()`ed handle aborts the task without `on_close`,
//! matching the local-events streams. The `StreamOpened`/`StreamClosed`
//! telemetry pair still closes on every ending — including that abort, which
//! never runs the task's tail but does drop it, so `StreamClosed` rides a
//! drop guard.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};

use parking_lot::Mutex;

use futures::Stream;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use xmtp_api_d14n::{BidiConnection, BidiTransport, OpenError, TransportError, V3Binding};
use xmtp_api_grpc::error::GrpcError;
use xmtp_common::{AbortHandle, MaybeSend, RetryableError, StreamHandle, StreamHandleError};
use xmtp_db::consent_record::ConsentState;
use xmtp_db::group::ConversationType;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_proto::api::ApiClientError;
use xmtp_proto::api_client::{XmtpMlsBidiStreams, XmtpMlsStreams};
use xmtp_proto::types::{GroupId, InstallationId};

use xmtp_common::Event;
use xmtp_macro::log_event;

use super::stream_messages::StreamGroupMessages;
use super::stream_router::{DEFAULT_STREAM_DEPTH, RouterError, RouterStream, StreamRouter};
use super::watchdog::run_watchdog_stream;
use super::{Result, StreamKind, SubscribeError};
use crate::Client;
use crate::context::XmtpSharedContext;
use crate::groups::MlsGroup;

/// Opt-in env var for the bidi streaming path (`1`/`true`/`yes`/`on`,
/// case-insensitive). Anything else — including unset — keeps the legacy
/// per-stream subscriptions.
pub const BIDI_STREAMS_ENABLED_ENV: &str = "XMTP_BIDI_STREAMS_ENABLED";

/// Whether callback streams should take the bidi path. The environment is
/// read once, at the first stream call — late enough that a process setting
/// the variable at init never races client construction, and never again,
/// since an env lookup scans the whole environ under a lock. Toggling the
/// variable mid-process has no effect.
pub(crate) fn bidi_streams_enabled() -> bool {
    static ENABLED: LazyLock<bool> = LazyLock::new(|| {
        std::env::var(BIDI_STREAMS_ENABLED_ENV)
            .map(|v| {
                matches!(
                    v.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    });
    *ENABLED
}

/// Process-shared per-destination wire state (see the module docs): the
/// transports, one per dialed URL, and the destinations whose backend
/// refused the bidi surface. Process-level because both facts outlive any
/// one client — clients dialing the same place share a wire, and a
/// backend's capability verdict holds for everyone dialing it.
struct SharedWires {
    /// One transport per destination, created at that destination's first
    /// stream. Entries are immortal (each holds the transport for the life
    /// of the process — see [`shared_transport`]); real processes dial one
    /// to three backends, so the map never meaningfully grows.
    transports: HashMap<String, BidiTransport<V3Binding>>,
    /// Destinations that refused the surface; their dispatches go straight
    /// to legacy.
    unsupported: HashSet<String>,
}

static SHARED_WIRES: LazyLock<Mutex<SharedWires>> = LazyLock::new(|| {
    Mutex::new(SharedWires {
        transports: HashMap::new(),
        unsupported: HashSet::new(),
    })
});

/// Whether a new stream to `host` should take the bidi path: the gate is on
/// and no earlier stream has latched that destination onto legacy.
pub fn bidi_streams_active(host: &str) -> bool {
    bidi_streams_enabled() && !SHARED_WIRES.lock().unsupported.contains(host)
}

/// Latch a destination onto the legacy streams. Called by the pump on a
/// backend refusal; tests pre-set it to exercise the latched dispatch.
pub(crate) fn latch_bidi_unsupported(host: &str) {
    SHARED_WIRES.lock().unsupported.insert(host.to_owned());
}

/// A latch's current state — test-only observability for the pump's
/// fallback branch. Gated like `router_callbacks_tests`, its only consumer.
#[cfg(all(test, not(feature = "d14n")))]
pub(crate) fn bidi_unsupported_latched(host: &str) -> bool {
    SHARED_WIRES.lock().unsupported.contains(host)
}

/// The transport count — test-only observability for the per-destination
/// keying.
#[cfg(all(test, not(feature = "d14n")))]
pub(crate) fn shared_transport_count() -> usize {
    SHARED_WIRES.lock().transports.len()
}

/// Whether a subscribe failure means the backend refuses the bidi surface —
/// the latch-worthy condition. Two shapes qualify:
///
/// - A wire open whose error chain carries the backend's own verdict that
///   the surface does not exist ([`open_is_backend_refusal`]) — that verdict
///   is buried under wrappers whose retryability cannot be trusted, so the
///   chain is walked instead of asking `is_retryable`.
/// - [`TransportError::Closed`] from a later `lease()`. A destination's
///   shared transport is immortal — [`SHARED_WIRES`] holds its handle for
///   the life of the process — so its ledger only ever dies by tombstoning
///   itself after an unretryable re-open: `Closed` is what the refusal looks
///   like to every stream after the one that saw it directly. Latching is
///   also the safe direction — the legacy path works everywhere.
///
/// A retryable open is a transient dial failure and a closed *router* is
/// client teardown — neither says anything about backend capability, so
/// neither latches.
pub(crate) fn is_bidi_unsupported(e: &RouterError) -> bool {
    match e {
        RouterError::Transport(TransportError::Open(open)) => open_is_backend_refusal(open),
        RouterError::Transport(TransportError::Closed) => true,
        RouterError::Transport(TransportError::Empty)
        | RouterError::Subscribe(_)
        | RouterError::Closed => false,
    }
}

/// Whether an open failure carries the backend refusing the bidi surface.
/// The verdict is buried, not top-level: a real v3 node without the surface
/// answers the dial with gRPC `UNIMPLEMENTED`, which arrives here as
/// `ApiClientError::ClientWithEndpoint` → `NetworkError` →
/// `GrpcError::Status` — and `GrpcError` reports itself blanket-retryable,
/// so the refusal must be recognized by shape, not by `is_retryable`. The
/// chain walk finds it wherever it sits.
pub(crate) fn open_is_backend_refusal(open: &OpenError) -> bool {
    let mut source = std::error::Error::source(open);
    while let Some(e) = source {
        if e.downcast_ref::<GrpcError>()
            .is_some_and(GrpcError::is_unimplemented)
        {
            return true;
        }
        // The in-process d14n and migration api clients refuse
        // `subscribe_bidi` outright with this variant, as does the transport
        // trait's default `bidi_stream` (no full-duplex support) — every
        // producer reachable from a bidi open is a capability verdict. The
        // v3 dial goes straight to the grpc client, middleware-free; if a
        // future d14n dial routes through middleware that maps local
        // failures to `OtherUnretryable`, the refusal needs its own shape.
        if matches!(
            e.downcast_ref::<ApiClientError>(),
            Some(ApiClientError::OtherUnretryable(_))
        ) {
            return true;
        }
        source = e.source();
    }
    false
}

/// Whether a subscribe failure is a dead end for THIS stream: a wire open no
/// redial can fix. Not necessarily the backend refusing the surface (a
/// garbled handshake response is unretryable too), so it must not latch the
/// process — but retrying the bidi arm is pointless for the stream that hit
/// it, which falls back to its legacy counterpart instead of error-closing.
pub(crate) fn is_bidi_dead_end(e: &RouterError) -> bool {
    matches!(e, RouterError::Transport(TransportError::Open(open)) if !open.is_retryable())
}

/// The shared transport for the destination `api` dials, created at that
/// destination's first stream (see the module docs and [`SHARED_WIRES`]).
///
/// A transport's ledger task is bound to the async runtime alive at its
/// first use — the one-runtime-per-process reality of mobile, node, and
/// agents. A process that tears its runtime down and starts another (some
/// non-`nextest` test harnesses) would find the cached transport dead;
/// `nextest`'s process-per-test model keeps tests clear of that.
pub(crate) fn shared_transport<C>(api: C) -> BidiTransport<V3Binding>
where
    C: XmtpMlsBidiStreams + Clone + Send + Sync + 'static,
    C::SubscribeStream: 'static,
{
    let host = api.host().to_owned();
    SHARED_WIRES
        .lock()
        .transports
        .entry(host)
        .or_insert_with_key(|host| {
            // Whoever streams to this destination first donates their api
            // client for the life of the process.
            tracing::info!(%host, "bidi: initializing the shared transport for a destination");
            BidiTransport::new(move |initial| {
                let api = api.clone();
                async move {
                    BidiConnection::open(&api, initial)
                        .await
                        .map_err(OpenError::new)
                }
            })
        })
        .clone()
}

/// Take the shared bidi wires — every destination's — off the network (the
/// `didEnterBackground` half of the app-lifecycle pair). Leases and wire
/// positions are kept; nothing reconnects until [`resume_bidi_streams`]. A
/// no-op when no transport was ever opened (gate off, or nothing ever
/// streamed).
pub async fn suspend_bidi_streams() -> Result<()> {
    let transports: Vec<_> = SHARED_WIRES.lock().transports.values().cloned().collect();
    settle_lifecycle(futures::future::join_all(transports.iter().map(|t| t.suspend())).await)
}

/// Bring the shared bidi wires back (`willEnterForeground`), resolving once
/// every destination's live wire has no catch-up wave left in flight. That
/// is a wire-level mark: replayed messages may still be decoding and storing
/// in the stream pipeline behind it; the wait is unbounded while the network
/// is down, and a concurrent fresh subscribe on a shared transport extends
/// it — the FFI exposure (follow-on) owes callers a processing-drain barrier
/// and a deadline before this can honestly serve as the background-fetch
/// primitive. A no-op when no transport was ever opened.
pub async fn resume_bidi_streams() -> Result<()> {
    let transports: Vec<_> = SHARED_WIRES.lock().transports.values().cloned().collect();
    settle_lifecycle(futures::future::join_all(transports.iter().map(|t| t.resume())).await)
}

/// Fold a lifecycle fan-out's results, driving EVERY destination before
/// reporting: `join_all` (never `try_join_all`) upstream, so one dead wire
/// cannot abandon the others mid-suspend/resume. A `Closed` transport is a
/// tombstoned destination (backend refused the surface) — permanently off
/// the network with its streams on legacy, so the lifecycle intent holds
/// for it vacuously; any other error is reported after the fan-out settles.
fn settle_lifecycle(results: Vec<std::result::Result<(), TransportError>>) -> Result<()> {
    for result in results {
        match result {
            Ok(()) | Err(TransportError::Closed) => {}
            Err(e) => {
                return Err(SubscribeError::from(
                    super::stream_router::RouterError::Transport(e),
                ));
            }
        }
    }
    Ok(())
}

/// Drain a router stream into a callback until it ends or the client shuts
/// down, then report the close once.
async fn pump<T>(
    mut stream: RouterStream<T>,
    cancel: CancellationToken,
    mut callback: impl FnMut(Result<T>),
    on_close: impl FnOnce(),
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            next = stream.next() => match next {
                Some(item) => callback(item),
                None => break,
            }
        }
    }
    on_close();
}

/// Emits the `StreamClosed` telemetry event when dropped. A guard rather
/// than a tail call: the bindings' closers end these streams by aborting the
/// task, and an aborted future never runs its tail — but it is dropped, so
/// the pair-closing event still fires.
struct StreamClosedGuard {
    kind: StreamKind,
    installation: InstallationId,
    armed: bool,
}

impl StreamClosedGuard {
    /// Hand the pair-closing emit to someone else. The legacy fallback
    /// stream emits its own `StreamClosed` on drop, so once it takes over
    /// this guard must go quiet or the pair would close twice.
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for StreamClosedGuard {
    fn drop(&mut self) {
        if self.armed {
            log_event!(Event::StreamClosed, self.installation, kind = ?self.kind);
        }
    }
}

/// Where a stream comes from: the per-stream identity [`pump_stream`]
/// needs, derived from ONE context so the latch destination can never
/// disagree with the client that built the subscribe and fallback futures.
pub(crate) struct StreamOrigin {
    pub(crate) kind: StreamKind,
    pub(crate) installation: InstallationId,
    /// The destination key ([`XmtpMlsBidiStreams::host`]) the stream
    /// latches on a backend refusal.
    pub(crate) host: String,
    pub(crate) cancel: CancellationToken,
}

impl StreamOrigin {
    fn new<Context>(kind: StreamKind, context: &Context) -> Self
    where
        Context: XmtpSharedContext,
        Context::ApiClient: XmtpMlsBidiStreams,
    {
        Self {
            kind,
            installation: context.installation_id(),
            host: context.api().api_client.host().to_owned(),
            cancel: context.cancellation_token().clone(),
        }
    }
}

/// One spawned bidi stream: emit `StreamOpened`, subscribe, signal ready,
/// then pump into the callback until the stream ends or the client shuts
/// down. Every entry point is this wrapper plus its subscribe future and a
/// `fallback` factory that builds its legacy counterpart stream. When the
/// subscribe fails with a backend refusal ([`is_bidi_unsupported`]) the task
/// latches the stream's destination ([`StreamOrigin::host`]) onto legacy;
/// on a refusal or a dead-end open
/// ([`is_bidi_dead_end`]) it keeps serving THIS stream by handing the
/// factory to the legacy watchdog runner ([`run_watchdog_stream`]) — the
/// same runner the legacy entry points spawn, stale-trip re-subscribe and
/// reconnect throttle included — so the caller that discovered the incapable
/// backend loses nothing. That also covers a startup burst: every stream
/// created before the latch tripped hits its own refusal and rides its own
/// fallback runner.
///
/// `on_close` fires on the subscribe-failure and natural-end paths only (an
/// aborted task must not invoke it — the caller asked for the end), exactly
/// like the local-events streams; `StreamClosed` fires on every ending via
/// the drop guard (disarmed on the fallback path, where the legacy stream
/// objects emit their own pair — and re-armed when the fallback fails at
/// startup, having built no stream object to pair the open).
pub(crate) fn pump_stream<T, S, FSub, FFut, FS>(
    origin: StreamOrigin,
    subscribe: S,
    fallback: FSub,
    callback: impl FnMut(Result<T>) + MaybeSend + 'static,
    on_close: impl FnOnce() + MaybeSend + 'static,
) -> impl StreamHandle<StreamOutput = Result<()>>
where
    T: MaybeSend + 'static,
    S: Future<Output = std::result::Result<RouterStream<T>, RouterError>> + MaybeSend + 'static,
    FSub: FnMut() -> FFut + MaybeSend + 'static,
    FFut: Future<Output = Result<FS>> + MaybeSend + 'static,
    FS: Stream<Item = Result<T>> + MaybeSend + 'static,
{
    let (tx, rx) = oneshot::channel();
    xmtp_common::spawn(Some(rx), async move {
        let StreamOrigin {
            kind,
            installation,
            host,
            cancel,
        } = origin;
        log_event!(Event::StreamOpened, installation, kind = ?kind);
        let mut closed = StreamClosedGuard {
            kind,
            installation,
            armed: true,
        };
        let stream = match subscribe.await {
            Err(e) if is_bidi_unsupported(&e) || is_bidi_dead_end(&e) => {
                if is_bidi_unsupported(&e) {
                    tracing::error!(
                        "bidi {kind:?} stream refused by the backend, \
                         latching {host} onto the legacy streams: {e}"
                    );
                    latch_bidi_unsupported(&host);
                } else {
                    // Not a capability verdict, but no redial of this open
                    // can succeed either — serve this one stream on legacy
                    // and leave this destination's latch to a cleaner failure.
                    tracing::warn!(
                        "bidi {kind:?} stream open failed unretryably, \
                         serving this stream on the legacy path: {e}"
                    );
                }
                closed.disarm();
                // The label the legacy entry point serving this shape logs
                // under — the fallback IS that entry point's stream.
                let label = match kind {
                    StreamKind::All => "stream_all_messages",
                    StreamKind::Conversations => "stream_conversations",
                    StreamKind::Messages => "stream_messages",
                };
                let ready = move || {
                    let _ = tx.send(());
                };
                let result =
                    run_watchdog_stream(cancel, label, fallback, ready, callback, on_close).await;
                if result.is_err() {
                    // A startup failure built no legacy stream object, so
                    // nothing else pairs the `StreamOpened` — re-arm.
                    closed.armed = true;
                }
                return result;
            }
            Err(e) => {
                // The subscribe itself failed: warn (the handle's result
                // carries the error but is rarely read), fire `on_close`
                // — legacy watchdog semantics — and the caller
                // re-subscribes from durable cursors.
                tracing::warn!("bidi {kind:?} stream failed to subscribe: {e}");
                on_close();
                return Err(e.into());
            }
            Ok(stream) => stream,
        };
        // The bidi path won: don't hold the fallback factory's captured
        // clones — or the latch host, needed only on the losing arms — for
        // the (long) life of this stream.
        drop((fallback, host));
        let _ = tx.send(());
        pump(stream, cancel, callback, on_close).await;
        tracing::debug!("bidi {kind:?} stream ended");
        Ok::<_, SubscribeError>(())
    })
}

/// The one `StreamHandle` shape a dispatcher can return: the two arms are
/// distinct opaque handle types, and an `if` cannot unify them without a
/// common carrier.
enum DispatchHandle<B, L> {
    Bidi(B),
    Legacy(L),
}

#[xmtp_common::async_trait]
impl<B, L, O> StreamHandle for DispatchHandle<B, L>
where
    B: StreamHandle<StreamOutput = O>,
    L: StreamHandle<StreamOutput = O>,
    O: MaybeSend,
{
    type StreamOutput = O;

    async fn wait_for_ready(&mut self) {
        match self {
            Self::Bidi(h) => h.wait_for_ready().await,
            Self::Legacy(h) => h.wait_for_ready().await,
        }
    }

    fn end(&self) {
        match self {
            Self::Bidi(h) => h.end(),
            Self::Legacy(h) => h.end(),
        }
    }

    async fn join(self) -> std::result::Result<O, StreamHandleError> {
        match self {
            Self::Bidi(h) => h.join().await,
            Self::Legacy(h) => h.join().await,
        }
    }

    async fn end_and_wait(&mut self) -> std::result::Result<O, StreamHandleError> {
        match self {
            Self::Bidi(h) => h.end_and_wait().await,
            Self::Legacy(h) => h.end_and_wait().await,
        }
    }

    fn abort_handle(&self) -> Box<dyn AbortHandle> {
        match self {
            Self::Bidi(h) => h.abort_handle(),
            Self::Legacy(h) => h.abort_handle(),
        }
    }
}

impl<Context> Client<Context>
where
    Context: XmtpSharedContext + 'static,
    Context::ApiClient: XmtpMlsBidiStreams + XmtpMlsStreams + Clone + Send + Sync + 'static,
    <Context::ApiClient as XmtpMlsBidiStreams>::SubscribeStream: 'static,
    Context::MlsStorage: 'static,
    Context::Db: 'static,
{
    /// This client's router over the process-shared bidi wire, created on
    /// first use.
    pub(crate) async fn stream_router(&self) -> &StreamRouter<Context> {
        self.stream_router
            .get_or_init(|| async {
                let api = self.context.api().api_client.clone();
                StreamRouter::new(self.context.clone(), shared_transport(api))
            })
            .await
    }

    /// The one call the bindings make for an all-messages stream: the bidi
    /// path when [`bidi_streams_active`], the legacy watchdog stream
    /// otherwise.
    pub fn stream_all_messages_with_callback_dispatch(
        client: Arc<Client<Context>>,
        conversation_type: Option<ConversationType>,
        consent_states: Option<Vec<ConsentState>>,
        callback: impl FnMut(Result<StoredGroupMessage>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>> {
        if bidi_streams_active(client.context.api().api_client.host()) {
            DispatchHandle::Bidi(Self::stream_all_messages_with_callback_bidi(
                client,
                conversation_type,
                consent_states,
                callback,
                on_close,
            ))
        } else {
            DispatchHandle::Legacy(Self::stream_all_messages_with_callback(
                client.context.clone(),
                conversation_type,
                consent_states,
                callback,
                on_close,
            ))
        }
    }

    /// Bidi-path counterpart of `stream_all_messages_with_callback`: every
    /// matching conversation's messages, decoded, over the shared wire. The
    /// stream grows with new conversations — the router's welcome
    /// auto-subscribe reflex leases each later-joined group's topic as its
    /// welcome arrives, no re-subscribe needed.
    pub(crate) fn stream_all_messages_with_callback_bidi(
        client: Arc<Client<Context>>,
        conversation_type: Option<ConversationType>,
        consent_states: Option<Vec<ConsentState>>,
        callback: impl FnMut(Result<StoredGroupMessage>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>> {
        let origin = StreamOrigin::new(StreamKind::All, &client.context);
        let fallback = {
            let client = client.clone();
            let consent_states = consent_states.clone();
            move || {
                let client = client.clone();
                let consent_states = consent_states.clone();
                async move {
                    client
                        .stream_all_messages_owned(conversation_type, consent_states)
                        .await
                }
            }
        };
        let subscribe = async move {
            let router = client.stream_router().await;
            router
                .stream_all_messages(conversation_type, consent_states, DEFAULT_STREAM_DEPTH)
                .await
        };
        pump_stream(origin, subscribe, fallback, callback, on_close)
    }

    /// The one call the bindings make for a conversations stream: the bidi
    /// path when [`bidi_streams_active`], the legacy watchdog stream
    /// otherwise.
    pub fn stream_conversations_with_callback_dispatch(
        client: Arc<Client<Context>>,
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
        callback: impl FnMut(Result<MlsGroup<Context>>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>> {
        if bidi_streams_active(client.context.api().api_client.host()) {
            DispatchHandle::Bidi(Self::stream_conversations_with_callback_bidi(
                client,
                conversation_type,
                include_duplicate_dms,
                callback,
                on_close,
            ))
        } else {
            DispatchHandle::Legacy(Self::stream_conversations_with_callback(
                client,
                conversation_type,
                callback,
                on_close,
                include_duplicate_dms,
            ))
        }
    }

    /// Bidi-path counterpart of `stream_conversations_with_callback`: new
    /// conversations — welcomes from the shared wire, plus this client's own
    /// locally-created groups via the `LocalEvents` broadcast (legacy
    /// parity).
    pub(crate) fn stream_conversations_with_callback_bidi(
        client: Arc<Client<Context>>,
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
        callback: impl FnMut(Result<MlsGroup<Context>>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>> {
        let origin = StreamOrigin::new(StreamKind::Conversations, &client.context);
        let fallback = {
            let client = client.clone();
            move || {
                let client = client.clone();
                async move {
                    client
                        .stream_conversations_owned(conversation_type, include_duplicate_dms)
                        .await
                }
            }
        };
        let subscribe = async move {
            let router = client.stream_router().await;
            router
                .stream_conversations(
                    conversation_type,
                    include_duplicate_dms,
                    None,
                    DEFAULT_STREAM_DEPTH,
                )
                .await
        };
        pump_stream(origin, subscribe, fallback, callback, on_close)
    }
}

/// The one call the bindings make for a single conversation's message
/// stream: the bidi path when [`bidi_streams_active`], the legacy watchdog
/// stream otherwise. Context-based like the arms it dispatches between (a
/// conversation object holds no client).
pub fn stream_conversation_messages_with_callback_dispatch<Context>(
    context: Context,
    group_id: GroupId,
    callback: impl FnMut(Result<StoredGroupMessage>) + MaybeSend + 'static,
    on_close: impl FnOnce() + MaybeSend + 'static,
) -> impl StreamHandle<StreamOutput = Result<()>>
where
    Context: XmtpSharedContext + 'static,
    Context::ApiClient: XmtpMlsBidiStreams + XmtpMlsStreams + Clone + Send + Sync + 'static,
    <Context::ApiClient as XmtpMlsBidiStreams>::SubscribeStream: 'static,
    Context::Db: 'static,
{
    if bidi_streams_active(context.api().api_client.host()) {
        DispatchHandle::Bidi(stream_conversation_messages_with_callback_bidi(
            context, group_id, callback, on_close,
        ))
    } else {
        DispatchHandle::Legacy(MlsGroup::stream_with_callback(
            context, group_id, callback, on_close,
        ))
    }
}

/// Bidi-path counterpart of [`MlsGroup::stream_with_callback`]: one
/// conversation's messages, decoded, over the shared wire.
///
/// Context-based (a conversation object holds no client), so it cannot reach
/// the client's cached router; it builds one scoped to this conversation
/// instead. That is safe by design: router state is per-stream and its task
/// exits with its last stream, dedup correctness lives in the transport's
/// per-lease positions, and the wire is the process-shared transport either
/// way.
pub(crate) fn stream_conversation_messages_with_callback_bidi<Context>(
    context: Context,
    group_id: GroupId,
    callback: impl FnMut(Result<StoredGroupMessage>) + MaybeSend + 'static,
    on_close: impl FnOnce() + MaybeSend + 'static,
) -> impl StreamHandle<StreamOutput = Result<()>>
where
    Context: XmtpSharedContext + 'static,
    Context::ApiClient: XmtpMlsBidiStreams + XmtpMlsStreams + Clone + Send + Sync + 'static,
    <Context::ApiClient as XmtpMlsBidiStreams>::SubscribeStream: 'static,
    Context::Db: 'static,
{
    let origin = StreamOrigin::new(StreamKind::Messages, &context);
    // `StreamGroupMessages::new_owned` directly: the legacy entry point for
    // this shape (`stream_messages_with_callback`) subscribes the same way —
    // context-based, there is no group object to call `stream_owned` on.
    let fallback = {
        let context = context.clone();
        move || {
            let context = context.clone();
            async move { StreamGroupMessages::new_owned(context, vec![group_id]).await }
        }
    };
    let subscribe = async move {
        let api = context.api().api_client.clone();
        let router = StreamRouter::new(context.clone(), shared_transport(api));
        router
            .stream_messages(vec![group_id], DEFAULT_STREAM_DEPTH)
            .await
    };
    pump_stream(origin, subscribe, fallback, callback, on_close)
}
