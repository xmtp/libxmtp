//! Native callback streams over the backend subscription router.
//!
//! The receiver owns the shared wires and their suspension intent
//! ([`shared_wires`]); this module keeps the app's suspend and resume calls.
//! Retryable wire failures keep leases alive and reconnect with backoff.
//! Each callback stream reports close on terminal failure or explicit close.

use tokio::sync::oneshot;

use xmtp_common::{MaybeSend, StreamHandle};
use xmtp_db::consent_record::ConsentState;
use xmtp_db::group::ConversationType;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_proto::api_client::{XmtpMlsBidiStreams, XmtpMlsStreams};
use xmtp_proto::types::{GroupId, InstallationId};

use xmtp_common::Event;
use xmtp_macro::log_event;

use super::{
    Result, StreamKind, SubscribeError,
    watchdog::{StreamCancel, close_reason},
};
use xmtp_mls_engine::groups::MlsGroup;
use xmtp_mls_engine::groups::context::XmtpSharedContext;
use xmtp_mls_engine::subscriptions::incoming::stream_api::shared_wires;

pub use xmtp_mls_engine::subscriptions::incoming::stream_api::shared_wires::ApiClientIdentity;
#[cfg(any(test, feature = "test-utils"))]
pub use xmtp_mls_engine::subscriptions::incoming::stream_api::shared_wires::shared_transport_count;

/// Suspend every shared wire and remember the intent for new wires.
pub async fn suspend_bidi_streams() -> Result<()> {
    shared_wires::suspend()
        .await
        .map_err(SubscribeError::Transport)
}

/// Resume shared wires. Reconnect and target processing continue in their tasks.
pub async fn resume_bidi_streams() -> Result<()> {
    shared_wires::resume();
    Ok(())
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
    pub(crate) cancel: StreamCancel,
}

impl StreamOrigin {
    fn new<Context: XmtpSharedContext>(kind: StreamKind, context: &Context) -> Self {
        Self {
            kind,
            installation: context.installation_id(),
            cancel: StreamCancel::new(context),
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
            biased;
            _ = cancel.cancelled() => return close_reason(&cancel, true, &mut callback),
            result = subscribe => match result {
                Ok(stream) => stream,
                Err(error) => {
                    close_reason(&cancel, true, &mut callback)?;
                    callback(Err(error));
                    return Ok(());
                }
            },
        };
        let _ = tx.send(());
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return close_reason(&cancel, true, &mut callback),
                next = stream.next() => match next {
                    Some(item) => {
                        // The connection can become blocked while the local read is in progress.
                        // Report it before another item reaches the callback.
                        close_reason(&cancel, true, &mut callback)?;
                        let terminal = item.is_err();
                        callback(item);
                        if terminal {
                            return Ok(());
                        }
                    },
                    None => return close_reason(&cancel, true, &mut callback),
                }
            }
        }
    };
    xmtp_common::spawn(Some(rx), xmtp_common::bind_task_hub(task))
}

/// `keep_alive` lives until the subscription opens.
pub(crate) fn stream_all_messages_with_callback_dispatch<C>(
    context: C,
    keep_alive: impl MaybeSend + 'static,
    conversation_type: Option<ConversationType>,
    consent_states: Option<Vec<ConsentState>>,
    callback: impl FnMut(Result<StoredGroupMessage>) + MaybeSend + 'static,
    on_close: impl FnOnce() + MaybeSend + 'static,
) -> impl StreamHandle<StreamOutput = Result<()>>
where
    C: XmtpSharedContext + 'static,
    C::ApiClient:
        XmtpMlsBidiStreams + XmtpMlsStreams + ApiClientIdentity + Clone + Send + Sync + 'static,
    <C::ApiClient as XmtpMlsBidiStreams>::SubscribeStream: 'static,
{
    let origin = StreamOrigin::new(StreamKind::All, &context);
    let subscribe = async move {
        let _keep_alive = keep_alive;
        super::stream_all::StreamAllMessages::new_owned(context, conversation_type, consent_states)
            .await
    };
    pump_stream(origin, subscribe, callback, on_close)
}

/// `keep_alive` lives until the subscription opens.
pub(crate) fn stream_conversations_with_callback_dispatch<C>(
    context: C,
    keep_alive: impl MaybeSend + 'static,
    conversation_type: Option<ConversationType>,
    include_duplicate_dms: bool,
    callback: impl FnMut(Result<MlsGroup<C>>) + MaybeSend + 'static,
    on_close: impl FnOnce() + MaybeSend + 'static,
) -> impl StreamHandle<StreamOutput = Result<()>>
where
    C: XmtpSharedContext + 'static,
    C::ApiClient:
        XmtpMlsBidiStreams + XmtpMlsStreams + ApiClientIdentity + Clone + Send + Sync + 'static,
    <C::ApiClient as XmtpMlsBidiStreams>::SubscribeStream: 'static,
{
    let origin = StreamOrigin::new(StreamKind::Conversations, &context);
    let subscribe = async move {
        let _keep_alive = keep_alive;
        super::stream_conversations::StreamConversations::new_owned(
            context,
            conversation_type,
            include_duplicate_dms,
            None,
        )
        .await
    };
    pump_stream(origin, subscribe, callback, on_close)
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
