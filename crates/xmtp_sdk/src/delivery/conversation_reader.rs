use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use futures::StreamExt;
use parking_lot::Mutex as SyncMutex;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use xmtp_mls::subscriptions::{
    SubscribeError, incoming::IncomingLease, stream_conversations::StreamConversations,
};
use xmtp_proto::types::ConversationType;

use crate::{
    ConnectionState, Conversation, ConversationKind, ErrorCategory, ErrorDetails, XmtpError,
    conversation::on_sdk_worker,
};

/// A pull reader for new stored conversations.
#[derive(uniffi::Object)]
pub struct ConversationReader {
    stream: Arc<Mutex<StreamConversations<xmtp_mls::MlsContext>>>,
    request_lock: Mutex<()>,
    pending: Arc<SyncMutex<Option<Conversation>>>,
    lease: Arc<IncomingLease>,
    closed: Arc<AtomicBool>,
    cancel: CancellationToken,
    context: xmtp_mls::MlsContext,
    client_key: u64,
}

impl ConversationReader {
    pub(crate) async fn open(
        context: xmtp_mls::MlsContext,
        kind: Option<ConversationKind>,
        client_key: u64,
    ) -> Result<Arc<Self>, XmtpError> {
        let conversation_type = kind.map(|kind| match kind {
            ConversationKind::Group => ConversationType::Group,
            ConversationKind::Dm => ConversationType::Dm,
        });
        let stream =
            StreamConversations::new_owned(context.clone(), conversation_type, false, None)
                .await
                .map_err(subscribe_error)?;
        let lease = stream.lease().expect("new stream owns its lease");
        Ok(Arc::new(Self {
            stream: Arc::new(Mutex::new(stream)),
            request_lock: Mutex::new(()),
            pending: Arc::new(SyncMutex::new(None)),
            lease,
            closed: Arc::new(AtomicBool::new(false)),
            cancel: CancellationToken::new(),
            context,
            client_key,
        }))
    }
}

impl Drop for ConversationReader {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.lease.close();
    }
}

#[xmtp_macro::sdk_export]
impl ConversationReader {
    pub async fn next(&self) -> Result<Option<Conversation>, XmtpError> {
        let _request = self.request_lock.lock().await;
        let request_cancel = CancellationToken::new();
        let _cancel_on_drop = super::CancelReadOnDrop(request_cancel.clone());
        let stream = self.stream.clone();
        let pending = self.pending.clone();
        let lease = self.lease.clone();
        let closed = self.closed.clone();
        let cancel = self.cancel.clone();
        let client_key = self.client_key;
        on_sdk_worker(self.context.clone(), async move {
            if closed.load(Ordering::Acquire) {
                return Ok(false);
            }
            let mut stream = stream.lock().await;
            if request_cancel.is_cancelled() {
                return Ok(false);
            }
            if pending.lock().is_some() {
                return Ok(true);
            }
            loop {
                let item = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return Ok(false),
                    _ = request_cancel.cancelled() => return Ok(false),
                    item = stream.next() => item,
                };
                if closed.load(Ordering::Acquire) {
                    return Ok(false);
                }
                match item {
                    Some(Ok(group)) => {
                        if let Some(conversation) =
                            Conversation::from_core(group, client_key).await?
                        {
                            let mut pending = pending.lock();
                            if closed.load(Ordering::Acquire) {
                                return Ok(false);
                            }
                            *pending = Some(conversation);
                            return Ok(true);
                        }
                    }
                    Some(Err(error)) => {
                        closed.store(true, Ordering::Release);
                        lease.close();
                        return Err(subscribe_error(error));
                    }
                    None => {
                        closed.store(true, Ordering::Release);
                        lease.close();
                        return Ok(false);
                    }
                }
            }
        })
        .await
        .and_then(|ready| {
            if !ready || self.closed.load(Ordering::Acquire) {
                return Ok(None);
            }
            let conversation = self.pending.lock().take();
            if self.closed.load(Ordering::Acquire) {
                return Ok(None);
            }
            conversation
                .map(Some)
                .ok_or_else(|| XmtpError::unknown("conversation handoff missing"))
        })
    }

    pub async fn end(&self) -> Result<(), XmtpError> {
        let stream = self.stream.clone();
        let lease = self.lease.clone();
        let closed = self.closed.clone();
        let cancel = self.cancel.clone();
        let pending = self.pending.clone();
        on_sdk_worker(self.context.clone(), async move {
            closed.store(true, Ordering::Release);
            cancel.cancel();
            lease.close();
            pending.lock().take();
            let _stream = stream.lock().await;
            Ok(())
        })
        .await
    }

    pub fn connection_state(&self) -> ConnectionState {
        self.lease.snapshot().connection.into()
    }

    pub async fn connection_state_changed(
        &self,
        previous: ConnectionState,
    ) -> Result<ConnectionState, XmtpError> {
        let lease = self.lease.clone();
        let cancel = self.cancel.clone();
        let mut changes = lease.subscribe_changes();
        on_sdk_worker(self.context.clone(), async move {
            loop {
                let current = lease.snapshot().connection.into();
                if current != previous || current == ConnectionState::Closed {
                    return Ok(current);
                }
                tokio::select! {
                    _ = cancel.cancelled() => return Ok(ConnectionState::Closed),
                    _ = changes.changed() => {},
                }
            }
        })
        .await
    }
}

fn subscribe_error(error: SubscribeError) -> XmtpError {
    let message = error.to_string();
    match error {
        SubscribeError::Configuration(cause) => super::configuration_error(&cause, message),
        SubscribeError::LocalDelivery(cause) => super::delivery_error(cause),
        SubscribeError::Db(_) | SubscribeError::Storage(_) => XmtpError::Storage(ErrorDetails {
            code: "storage".into(),
            category: ErrorCategory::Storage,
            retryable: true,
            message,
        }),
        other => XmtpError::unknown(other),
    }
}
