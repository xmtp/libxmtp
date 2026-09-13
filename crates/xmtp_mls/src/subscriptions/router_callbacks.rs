//! Native callback streams over the backend subscription router.
//!
//! Clients share a wire only when they share an API client Arc and host.
//! The registry retains the API client, so its address cannot be reused.
//! Retryable wire failures keep leases alive and reconnect with backoff.
//! Each callback stream reports close on terminal failure or explicit close.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use parking_lot::Mutex;

use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use xmtp_api_backend::{BackendBinding, BidiConnection, BidiTransport, OpenError, TransportError};
use xmtp_common::{MaybeSend, StreamHandle};
use xmtp_db::consent_record::ConsentState;
use xmtp_db::group::ConversationType;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_proto::api_client::{XmtpMlsBidiStreams, XmtpMlsStreams};
use xmtp_proto::types::{GroupId, InstallationId};

use xmtp_common::Event;
use xmtp_macro::log_event;

use super::{Result, StreamKind, SubscribeError};
use crate::Client;
use crate::context::XmtpSharedContext;
use crate::groups::MlsGroup;

/// Stable identity of the Arc that owns a backend API client.
pub trait ApiClientIdentity {
    fn api_client_identity(&self) -> usize;
}

impl<C: ?Sized> ApiClientIdentity for Arc<C> {
    fn api_client_identity(&self) -> usize {
        Arc::as_ptr(self).cast::<()>() as usize
    }
}

type WireKey = (String, usize);

struct SharedWires {
    transports: HashMap<WireKey, BidiTransport<BackendBinding>>,
    suspend_requested: bool,
}

static SHARED_WIRES: LazyLock<Mutex<SharedWires>> = LazyLock::new(|| {
    Mutex::new(SharedWires {
        transports: HashMap::new(),
        suspend_requested: false,
    })
});

/// Prevent automatic unary receipt from bypassing native stream suspension.
pub(crate) fn bidi_streams_suspended() -> bool {
    SHARED_WIRES.lock().suspend_requested
}

#[cfg(test)]
pub(crate) fn shared_transport_count() -> usize {
    SHARED_WIRES.lock().transports.len()
}

/// The shared transport for the destination `api` dials, created at that
/// destination's first stream (see the module docs and [`SHARED_WIRES`]).
///
/// A transport's ledger task is bound to the async runtime alive at its
/// first use — the one-runtime-per-process reality of mobile, node, and
/// agents. A process that tears its runtime down and starts another (some
/// non-`nextest` test harnesses) would find the cached transport dead;
/// `nextest`'s process-per-test model keeps tests clear of that.
pub(crate) fn shared_transport<C>(api: C) -> BidiTransport<BackendBinding>
where
    C: XmtpMlsBidiStreams + ApiClientIdentity + Clone + Send + Sync + 'static,
    C::SubscribeStream: 'static,
{
    let key = (api.host().to_owned(), api.api_client_identity());
    let mut wires = SHARED_WIRES.lock();
    // A transport created while the app is backgrounded is born suspended —
    // its first lease parks instead of dialing (see [`SharedWires`]).
    let born_suspended = wires.suspend_requested;
    wires
        .transports
        .entry(key)
        .or_insert_with_key(|(host, _)| {
            // Whoever streams to this destination first donates their api
            // client for the life of the process.
            tracing::info!(
                %host,
                suspended = born_suspended,
                "bidi: initializing the shared transport for a destination"
            );
            BidiTransport::new(
                move |initial| {
                    let api = api.clone();
                    async move {
                        BidiConnection::open(&api, initial)
                            .await
                            .map_err(OpenError::new)
                    }
                },
                born_suspended,
            )
        })
        .clone()
}

/// Suspend every shared wire and remember the intent for new wires.
pub async fn suspend_bidi_streams() -> Result<()> {
    // Enqueue each transport's command UNDER the same lock that flips the
    // process flag, so a concurrent resume can't interleave its command send
    // between our flag flip and ours — the actor then sees Suspend/Resume in
    // the order the lock ordered the flags, not in scheduler order. The push is
    // synchronous; only the reply await below runs outside the lock.
    let (wires, pending): (Vec<_>, Vec<_>) = {
        let mut shared = SHARED_WIRES.lock();
        shared.suspend_requested = true;
        shared
            .transports
            .iter()
            .map(|(host, t)| ((host.clone(), t.clone()), t.enqueue_suspend()))
            .unzip()
    };
    settle_lifecycle(&wires, await_lifecycle_acks(pending).await)
}

/// Resume shared wires. Reconnect and target processing continue in their tasks.
pub async fn resume_bidi_streams() -> Result<()> {
    // Enqueue under the flag lock, same as [`suspend_bidi_streams`]: the
    // command order must match the flag order under all interleavings. The
    // reply receivers are dropped — fire-and-forget — and an `Err` from a
    // closed (tombstoned) transport is dropped with them: a destination
    // permanently off the network is vacuously resumed.
    let mut shared = SHARED_WIRES.lock();
    shared.suspend_requested = false;
    for t in shared.transports.values() {
        let _ = t.enqueue_resume();
    }
    Ok(())
}

/// Await every enqueued lifecycle reply, mapping a dropped sender (a closed
/// transport) to `Closed`. The command pushes already happened under the
/// registry lock ([`suspend_bidi_streams`]); only this await runs outside it,
/// so a slow catch-up on one destination never delays another's command.
async fn await_lifecycle_acks(
    pending: Vec<std::result::Result<tokio::sync::oneshot::Receiver<()>, TransportError>>,
) -> Vec<std::result::Result<(), TransportError>> {
    futures::future::join_all(pending.into_iter().map(|reply| async move {
        match reply {
            Ok(rx) => rx.await.map_err(|_| TransportError::Closed),
            Err(e) => Err(e),
        }
    }))
    .await
}

/// Wait for every wire before reporting a lifecycle failure.
fn settle_lifecycle(
    wires: &[(WireKey, BidiTransport<BackendBinding>)],
    results: Vec<std::result::Result<(), TransportError>>,
) -> Result<()> {
    let mut first_error = None;
    for (_, result) in wires.iter().zip(results) {
        match result {
            Ok(()) => {}
            Err(TransportError::Closed) => {}
            Err(e) => first_error = first_error.or(Some(e)),
        }
    }
    match first_error {
        None => Ok(()),
        Some(e) => Err(SubscribeError::Transport(e)),
    }
}

/// Close telemetry and callback also run when the stream task is aborted.
struct StreamClosedGuard<F: FnOnce()> {
    kind: StreamKind,
    installation: InstallationId,
    on_close: Option<F>,
}

impl<F: FnOnce()> Drop for StreamClosedGuard<F> {
    fn drop(&mut self) {
        log_event!(Event::StreamClosed, self.installation, kind = ?self.kind);
        if let Some(on_close) = self.on_close.take() {
            on_close();
        }
    }
}

pub(crate) struct StreamOrigin {
    pub(crate) kind: StreamKind,
    pub(crate) installation: InstallationId,
    pub(crate) cancel: CancellationToken,
}

impl StreamOrigin {
    fn new<Context: XmtpSharedContext>(kind: StreamKind, context: &Context) -> Self {
        Self {
            kind,
            installation: context.installation_id(),
            cancel: context.cancellation_token().clone(),
        }
    }
}

/// Deliver local stream items. The next poll acknowledges the preceding callback return.
pub(crate) fn pump_stream<T, S, St>(
    origin: StreamOrigin,
    subscribe: S,
    mut callback: impl FnMut(Result<T>) + MaybeSend + 'static,
    on_close: impl FnOnce() + MaybeSend + 'static,
) -> impl StreamHandle<StreamOutput = Result<()>>
where
    T: MaybeSend + 'static,
    St: futures::Stream<Item = Result<T>> + MaybeSend + Unpin + 'static,
    S: Future<Output = Result<St>> + MaybeSend + 'static,
{
    use futures::StreamExt;
    let (tx, rx) = oneshot::channel();
    let task = async move {
        let StreamOrigin {
            kind,
            installation,
            cancel,
        } = origin;
        log_event!(Event::StreamOpened, installation, kind = ?kind);
        let _closed = StreamClosedGuard {
            kind,
            installation,
            on_close: Some(on_close),
        };
        let mut stream = tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            result = subscribe => result?,
        };
        let _ = tx.send(());
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                next = stream.next() => match next {
                    Some(item) => callback(item),
                    None => break,
                }
            }
        }
        Ok(())
    };
    xmtp_common::spawn(Some(rx), xmtp_common::bind_task_hub(task))
}

impl<C> Client<C>
where
    C: XmtpSharedContext + 'static,
    C::ApiClient:
        XmtpMlsBidiStreams + XmtpMlsStreams + ApiClientIdentity + Clone + Send + Sync + 'static,
    <C::ApiClient as XmtpMlsBidiStreams>::SubscribeStream: 'static,
{
    pub fn stream_all_messages_with_callback_dispatch(
        client: Arc<Client<C>>,
        conversation_type: Option<ConversationType>,
        consent_states: Option<Vec<ConsentState>>,
        callback: impl FnMut(Result<StoredGroupMessage>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>> {
        let origin = StreamOrigin::new(StreamKind::All, &client.context);
        let subscribe = async move {
            super::stream_all::StreamAllMessages::new_owned(
                client.context.clone(),
                conversation_type,
                consent_states,
            )
            .await
        };
        pump_stream(origin, subscribe, callback, on_close)
    }

    pub fn stream_conversations_with_callback_dispatch(
        client: Arc<Client<C>>,
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
        callback: impl FnMut(Result<MlsGroup<C>>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>> {
        let origin = StreamOrigin::new(StreamKind::Conversations, &client.context);
        let subscribe = async move {
            super::stream_conversations::StreamConversations::new_owned(
                client.context.clone(),
                conversation_type,
                include_duplicate_dms,
                None,
            )
            .await
        };
        pump_stream(origin, subscribe, callback, on_close)
    }
}

pub fn stream_conversation_messages_with_callback_dispatch<C>(
    context: C,
    group_id: GroupId,
    callback: impl FnMut(Result<StoredGroupMessage>) + MaybeSend + 'static,
    on_close: impl FnOnce() + MaybeSend + 'static,
) -> impl StreamHandle<StreamOutput = Result<()>>
where
    C: XmtpSharedContext + 'static,
    C::ApiClient:
        XmtpMlsBidiStreams + XmtpMlsStreams + ApiClientIdentity + Clone + Send + Sync + 'static,
    <C::ApiClient as XmtpMlsBidiStreams>::SubscribeStream: 'static,
{
    let origin = StreamOrigin::new(StreamKind::Messages, &context);
    let subscribe = async move {
        super::stream_messages::StreamGroupMessages::new_owned(context, vec![group_id]).await
    };
    pump_stream(origin, subscribe, callback, on_close)
}
