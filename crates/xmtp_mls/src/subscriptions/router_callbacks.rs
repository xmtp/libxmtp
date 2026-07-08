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
//! its api client — connection, auth middleware and all — to the opener,
//! which assumes one backend and one set of wire credentials per process:
//! true for mobile, agents, and `nextest`'s process-per-test runs. (Client
//! *identity* is not wire state — sibling clients' topics multiplex fine —
//! but a process mixing differently-authenticated api clients would ride
//! the donor's credentials.)
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
//! failure fires `on_close()` and carries the error out through the handle's
//! result — legacy watchdog semantics, so `end_and_wait()` distinguishes a
//! startup failure from a clean end. `on_close` fires once
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
use xmtp_db::group::{ConversationType, GroupQueryArgs};
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_db::prelude::*;
use xmtp_proto::api_client::XmtpMlsBidiStreams;

use super::stream_router::{DEFAULT_STREAM_DEPTH, RouterStream, StreamRouter};
use super::{Result, SubscribeError, SyncWorkerEvent};
use crate::Client;
use crate::context::XmtpSharedContext;
use crate::groups::MlsGroup;
use crate::groups::welcome_sync::WelcomeService;

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
                        .map_err(|e| Box::new(e) as OpenError)
                }
            })
        })
        .clone()
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
    /// matching conversation's messages, decoded, over the shared wire.
    ///
    /// Interim scope: covers the groups known at subscribe time. A
    /// conversation joined afterwards reaches this stream on re-subscribe,
    /// until the welcome auto-subscribe reflex lands.
    pub fn stream_all_messages_with_callback_bidi(
        client: Arc<Client<Context>>,
        conversation_type: Option<ConversationType>,
        consent_states: Option<Vec<ConsentState>>,
        mut callback: impl FnMut(Result<StoredGroupMessage>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>> {
        let (tx, rx) = oneshot::channel();
        xmtp_common::spawn(Some(rx), async move {
            let cancel = client.context.cancellation_token().clone();
            let subscribed = async {
                // Same seeding as the legacy stream: close the welcome gap,
                // then subscribe every matching group. A conversation joined
                // after this point reaches the stream on re-subscribe (until
                // the welcome auto-subscribe reflex lands).
                WelcomeService::new(&client.context).sync_welcomes().await?;
                let groups = client.context.db().find_groups(GroupQueryArgs {
                    conversation_type,
                    consent_states: consent_states.clone(),
                    include_duplicate_dms: true,
                    include_sync_groups: conversation_type
                        .map(|ct| matches!(ct, ConversationType::Sync))
                        .unwrap_or(true),
                    ..Default::default()
                })?;
                // Sync groups are subscribed (their traffic nudges the
                // device-sync worker) but their messages are intercepted
                // below, exactly like the legacy stream — internal payloads
                // must not surface as conversation messages.
                let sync_groups: Vec<Vec<u8>> = groups
                    .iter()
                    .filter(|g| matches!(g.conversation_type, ConversationType::Sync))
                    .map(|g| g.id.to_vec())
                    .collect();
                let ids: Vec<GroupId> = groups.into_iter().map(|g| g.id).collect();
                if ids.is_empty() {
                    // Nothing matches (fresh account, or an empty filter):
                    // the transport refuses an empty lease, and legacy stays
                    // open here — so stay open with nothing subscribed.
                    // Deliveries begin on re-subscribe (interim scope above).
                    return Ok::<_, SubscribeError>(None);
                }
                let router = client.stream_router().await;
                let stream = router.stream_messages(ids, DEFAULT_STREAM_DEPTH).await?;
                Ok(Some((stream, sync_groups)))
            };
            let (stream, sync_groups) = match subscribed.await {
                Ok(Some(subscription)) => subscription,
                Ok(None) => {
                    let _ = tx.send(());
                    cancel.cancelled().await;
                    on_close();
                    return Ok(());
                }
                Err(e) => {
                    // The subscribe itself failed: `on_close` fires and the
                    // handle's result carries the error (legacy watchdog
                    // semantics); the caller re-subscribes from durable
                    // cursors.
                    on_close();
                    return Err(e);
                }
            };
            let _ = tx.send(());
            let worker_events = client.context.worker_events().clone();
            let callback = move |message: Result<StoredGroupMessage>| {
                if let Ok(m) = &message
                    && sync_groups
                        .iter()
                        .any(|id| id.as_slice() == m.group_id.as_slice())
                {
                    let _ = worker_events.send(SyncWorkerEvent::NewSyncGroupMsg);
                    return;
                }
                callback(message);
            };
            pump(stream, cancel, callback, on_close).await;
            tracing::debug!("bidi `stream_all_messages` ended");
            Ok::<_, SubscribeError>(())
        })
    }

    /// Bidi-path counterpart of `stream_conversations_with_callback`: new
    /// conversations from the welcome topic, over the shared wire.
    ///
    /// Interim scope: welcome topic only — a conversation created *locally*
    /// by this client does not surface here (legacy multiplexes
    /// `LocalEvents::NewGroup` for that). Parity arrives with the reflex
    /// work, which fans local new-group events into both stream kinds.
    pub fn stream_conversations_with_callback_bidi(
        client: Arc<Client<Context>>,
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
        callback: impl FnMut(Result<MlsGroup<Context>>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>> {
        let (tx, rx) = oneshot::channel();
        xmtp_common::spawn(Some(rx), async move {
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
                    on_close();
                    return Err(e.into());
                }
            };
            let _ = tx.send(());
            pump(stream, cancel, callback, on_close).await;
            tracing::debug!("bidi `stream_conversations` ended");
            Ok::<_, SubscribeError>(())
        })
    }
}
