//! Callback adapters over the XIP-83 [`StreamRouter`] — the pull→push bridge
//! the bindings call, plus the pieces that decide *whether* and *where* the
//! bidi path runs.
//!
//! ## One wire per process
//!
//! Every client in the process shares ONE [`BidiTransport`]: topics from all
//! of them multiplex onto a single bidi stream, and the transport's leases
//! keep them apart (welcome topics are per-installation; a group topic two
//! clients share is subscribed once and fanned out). One client per process —
//! mobile — makes this invisible; a process running many clients (agents)
//! gets O(1) wires instead of O(clients). The first client to stream donates
//! its api client to the opener, which assumes one backend per process: true
//! for mobile, agents, and `nextest`'s process-per-test runs. Auth is not at
//! stake — the shared wire is receive-only (publishes keep each client's own
//! api client and whatever authorization it carries), and on v3 the donated
//! client attaches only version-attribution headers. The hazard of mixing
//! backends is misrouting: a sibling client pointed elsewhere would lease
//! its topics on the donor's wire, subscribe successfully, and receive
//! nothing — hence the init log below.
//!
//! ## The gate
//!
//! The bidi path is opt-in via [`BIDI_STREAMS_ENABLED_ENV`], read once at
//! the first stream call: mobile apps set it at process init, agents in
//! their deploy env. Unset (or anything but a truthy value) keeps the legacy
//! streams. Dispatch on the gate lives with the bindings; the
//! `*_with_callback_bidi` entry points here are the bidi arm.
//!
//! ## Pump semantics
//!
//! Each stream is one spawned task: subscribe (ready fires once the lease is
//! registered), then drain the router stream into the callback. A subscribe
//! failure warns, fires `on_close()`, and carries the error out through the
//! handle's result — legacy watchdog semantics; the warn is the reliable
//! trace, since the bindings' closers rarely read the result. `on_close` fires once
//! at natural end — the wire-facing lease survives reconnects and suspends
//! (transport docs), so a natural end means this consumer fell behind and
//! was dropped, or the client shut down; re-subscribing recovers from
//! durable cursors. An `end()`ed handle aborts the task without `on_close`,
//! matching the local-events streams.

use std::sync::{Arc, LazyLock, OnceLock};

use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use xmtp_api_d14n::{BidiConnection, BidiTransport, OpenError, V3Binding};
use xmtp_common::{MaybeSend, StreamHandle};
use xmtp_db::consent_record::ConsentState;
use xmtp_db::group::ConversationType;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_proto::api_client::XmtpMlsBidiStreams;
use xmtp_proto::types::GroupId;

use xmtp_common::Event;
use xmtp_macro::log_event;

use super::stream_router::{DEFAULT_STREAM_DEPTH, RouterStream, StreamRouter};
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
pub fn bidi_streams_enabled() -> bool {
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

/// The process-wide transport (see the module docs). `OnceLock` rather than
/// per-client state: the whole point is that clients share the wire.
///
/// Its ledger task is bound to the async runtime alive at first use — the
/// one-runtime-per-process reality of mobile, node, and agents. A process
/// that tears its runtime down and starts another (some non-`nextest` test
/// harnesses) would find the cached transport dead; `nextest`'s
/// process-per-test model keeps tests clear of that.
static SHARED_TRANSPORT: OnceLock<BidiTransport<V3Binding>> = OnceLock::new();

fn shared_transport<C>(api: C) -> BidiTransport<V3Binding>
where
    C: XmtpMlsBidiStreams + Clone + Send + Sync + 'static,
    C::SubscribeStream: 'static,
{
    SHARED_TRANSPORT
        .get_or_init(move || {
            // Whoever streams first donates their api client for the life of
            // the process — log it, so a mixed-backend accident is findable.
            tracing::info!("bidi: initializing the process-shared transport");
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

/// Take the shared bidi wire off the network (the `didEnterBackground` half
/// of the app-lifecycle pair). Leases and wire positions are kept; nothing
/// reconnects until [`resume_bidi_streams`]. A no-op when the transport was
/// never opened (gate off, or nothing ever streamed).
pub async fn suspend_bidi_streams() -> Result<()> {
    let Some(transport) = SHARED_TRANSPORT.get() else {
        return Ok(());
    };
    transport
        .suspend()
        .await
        .map_err(|e| super::stream_router::RouterError::Transport(e).into())
}

/// Bring the shared bidi wire back (`willEnterForeground`), resolving once
/// the wire's resume wave has caught up. That is a wire-level mark: replayed
/// messages may still be decoding and storing in the stream pipeline behind
/// it, and the wait is unbounded while the network is down — the FFI
/// exposure (follow-on) owes callers a processing-drain barrier and a
/// deadline before this can honestly serve as the background-fetch
/// primitive. A no-op when the transport was never opened.
pub async fn resume_bidi_streams() -> Result<()> {
    let Some(transport) = SHARED_TRANSPORT.get() else {
        return Ok(());
    };
    transport
        .resume()
        .await
        .map_err(|e| super::stream_router::RouterError::Transport(e).into())
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

impl<Context> Client<Context>
where
    Context: XmtpSharedContext + 'static,
    Context::ApiClient: XmtpMlsBidiStreams + Clone + Send + Sync + 'static,
    <Context::ApiClient as XmtpMlsBidiStreams>::SubscribeStream: 'static,
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

    /// Bidi-path counterpart of `stream_all_messages_with_callback`: every
    /// matching conversation's messages, decoded, over the shared wire. The
    /// stream grows with new conversations — the router's welcome
    /// auto-subscribe reflex leases each later-joined group's topic as its
    /// welcome arrives, no re-subscribe needed.
    pub fn stream_all_messages_with_callback_bidi(
        client: Arc<Client<Context>>,
        conversation_type: Option<ConversationType>,
        consent_states: Option<Vec<ConsentState>>,
        callback: impl FnMut(Result<StoredGroupMessage>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>> {
        let (tx, rx) = oneshot::channel();
        xmtp_common::spawn(Some(rx), async move {
            let installation = client.context.installation_id();
            log_event!(Event::StreamOpened, installation, kind = ?StreamKind::All);
            let cancel = client.context.cancellation_token().clone();
            let router = client.stream_router().await;
            let stream = match router
                .stream_all_messages(conversation_type, consent_states, DEFAULT_STREAM_DEPTH)
                .await
            {
                Ok(stream) => stream,
                Err(e) => {
                    // The subscribe itself failed: warn (the handle's result
                    // carries the error but is rarely read), fire `on_close`
                    // — legacy watchdog semantics — and the caller
                    // re-subscribes from durable cursors.
                    tracing::warn!("bidi `stream_all_messages` failed to subscribe: {e}");
                    log_event!(Event::StreamClosed, installation, kind = ?StreamKind::All);
                    on_close();
                    return Err(e.into());
                }
            };
            let _ = tx.send(());
            pump(stream, cancel, callback, on_close).await;
            tracing::debug!("bidi `stream_all_messages` ended");
            log_event!(Event::StreamClosed, installation, kind = ?StreamKind::All);
            Ok::<_, SubscribeError>(())
        })
    }

    /// Bidi-path counterpart of `stream_conversations_with_callback`: new
    /// conversations — welcomes from the shared wire, plus this client's own
    /// locally-created groups via the `LocalEvents` broadcast (legacy
    /// parity).
    pub fn stream_conversations_with_callback_bidi(
        client: Arc<Client<Context>>,
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
        callback: impl FnMut(Result<MlsGroup<Context>>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>> {
        let (tx, rx) = oneshot::channel();
        xmtp_common::spawn(Some(rx), async move {
            let installation = client.context.installation_id();
            log_event!(Event::StreamOpened, installation, kind = ?StreamKind::Conversations);
            let cancel = client.context.cancellation_token().clone();
            let router = client.stream_router().await;
            let stream = match router
                .stream_conversations(
                    conversation_type,
                    include_duplicate_dms,
                    None,
                    DEFAULT_STREAM_DEPTH,
                )
                .await
            {
                Ok(stream) => stream,
                Err(e) => {
                    tracing::warn!("bidi `stream_conversations` failed to subscribe: {e}");
                    log_event!(Event::StreamClosed, installation, kind = ?StreamKind::Conversations);
                    on_close();
                    return Err(e.into());
                }
            };
            let _ = tx.send(());
            pump(stream, cancel, callback, on_close).await;
            tracing::debug!("bidi `stream_conversations` ended");
            log_event!(Event::StreamClosed, installation, kind = ?StreamKind::Conversations);
            Ok::<_, SubscribeError>(())
        })
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
pub fn stream_conversation_messages_with_callback_bidi<Context>(
    context: Context,
    group_id: GroupId,
    callback: impl FnMut(Result<StoredGroupMessage>) + MaybeSend + 'static,
    on_close: impl FnOnce() + MaybeSend + 'static,
) -> impl StreamHandle<StreamOutput = Result<()>>
where
    Context: XmtpSharedContext + 'static,
    Context::ApiClient: XmtpMlsBidiStreams + Clone + Send + Sync + 'static,
    <Context::ApiClient as XmtpMlsBidiStreams>::SubscribeStream: 'static,
{
    let (tx, rx) = oneshot::channel();
    xmtp_common::spawn(Some(rx), async move {
        let installation = context.installation_id();
        log_event!(Event::StreamOpened, installation, kind = ?StreamKind::Messages);
        let cancel = context.cancellation_token().clone();
        let api = context.api().api_client.clone();
        let router = StreamRouter::new(context.clone(), shared_transport(api));
        let stream = match router
            .stream_messages(vec![group_id], DEFAULT_STREAM_DEPTH)
            .await
        {
            Ok(stream) => stream,
            Err(e) => {
                tracing::warn!("bidi `stream_conversation_messages` failed to subscribe: {e}");
                log_event!(Event::StreamClosed, installation, kind = ?StreamKind::Messages);
                on_close();
                return Err(e.into());
            }
        };
        let _ = tx.send(());
        pump(stream, cancel, callback, on_close).await;
        tracing::debug!("bidi `stream_conversation_messages` ended");
        log_event!(Event::StreamClosed, installation, kind = ?StreamKind::Messages);
        Ok::<_, SubscribeError>(())
    })
}
