use futures::{Stream, StreamExt};
use process_welcome::ProcessWelcomeFuture;
use prost::Message;
use std::{collections::HashSet, sync::Arc};
use tokio::sync::{broadcast, oneshot};
use tokio_stream::wrappers::BroadcastStream;

use tracing::instrument;
use xmtp_db::prelude::*;
use xmtp_proto::{api_client::XmtpMlsStreams, xmtp::mls::api::v1::WelcomeMessage};

use process_welcome::ProcessWelcomeResult;
use stream_all::StreamAllMessages;
use stream_conversations::{StreamConversations, WelcomeOrGroup};

pub(super) mod process_message;
pub(super) mod process_welcome;
mod stream_all;
mod stream_conversations;
pub(crate) mod stream_messages;
mod stream_utils;

use crate::{
    Client,
    context::XmtpSharedContext,
    groups::{
        GroupError, MlsGroup, device_sync::preference_sync::PreferenceUpdate,
        mls_sync::GroupMessageProcessingError,
    },
};
use thiserror::Error;
use xmtp_common::{RetryableError, StreamHandle, retryable};
use xmtp_db::{
    NotFound, StorageError,
    consent_record::{ConsentState, StoredConsentRecord},
    group::ConversationType,
    group_message::StoredGroupMessage,
};

pub(crate) type Result<T> = std::result::Result<T, SubscribeError>;

#[derive(Debug, Error)]
pub enum LocalEventError {
    #[error("Unable to send event: {0}")]
    Send(String),
}

impl RetryableError for LocalEventError {
    fn is_retryable(&self) -> bool {
        true
    }
}

/// Events local to this client
/// are broadcast across all senders/receivers of streams
#[derive(Debug, Clone)]
pub enum LocalEvents {
    // a new group was created
    NewGroup(Vec<u8>),
    PreferencesChanged(Vec<PreferenceUpdate>),
}

#[derive(Debug, Clone)]
pub enum SyncWorkerEvent {
    NewSyncGroupFromWelcome(Vec<u8>),
    NewSyncGroupMsg,
    // The sync worker will auto-sync these with other devices.
    SyncPreferences(Vec<PreferenceUpdate>),
    CycleHMAC,

    // TODO: Device Sync V1 below - Delete when V1 is deleted
    Request { message_id: Vec<u8> },
    Reply { message_id: Vec<u8> },
}

impl LocalEvents {
    fn group_filter(self) -> Option<Vec<u8>> {
        use LocalEvents::*;
        // this is just to protect against any future variants
        match self {
            NewGroup(c) => Some(c),
            _ => None,
        }
    }

    fn consent_filter(self) -> Option<Vec<StoredConsentRecord>> {
        match self {
            Self::PreferencesChanged(updates) => {
                let updates = updates
                    .into_iter()
                    .filter_map(|pu| match pu {
                        PreferenceUpdate::Consent(cr) => Some(cr),
                        _ => None,
                    })
                    .collect();
                Some(updates)
            }

            _ => None,
        }
    }

    fn preference_filter(self) -> Option<Vec<PreferenceUpdate>> {
        match self {
            Self::PreferencesChanged(updates) => {
                let updates = updates
                    .into_iter()
                    .filter_map(|pu| match pu {
                        PreferenceUpdate::Consent(_) => None,
                        _ => Some(pu),
                    })
                    .collect();
                Some(updates)
            }
            _ => None,
        }
    }
}

pub(crate) trait StreamMessages {
    fn stream_consent_updates(self) -> impl Stream<Item = Result<Vec<StoredConsentRecord>>>;
    fn stream_preference_updates(self) -> impl Stream<Item = Result<Vec<PreferenceUpdate>>>;
}

impl StreamMessages for broadcast::Receiver<LocalEvents> {
    #[instrument(level = "trace", skip_all)]
    fn stream_consent_updates(self) -> impl Stream<Item = Result<Vec<StoredConsentRecord>>> {
        BroadcastStream::new(self).filter_map(|event| async {
            xmtp_common::optify!(event, "Missed message due to event queue lag")
                .and_then(LocalEvents::consent_filter)
                .map(Result::Ok)
        })
    }

    #[instrument(level = "trace", skip_all)]
    fn stream_preference_updates(self) -> impl Stream<Item = Result<Vec<PreferenceUpdate>>> {
        BroadcastStream::new(self).filter_map(|event| async {
            xmtp_common::optify!(event, "Missed message due to event queue lag")
                .and_then(LocalEvents::preference_filter)
                .map(Result::Ok)
        })
    }
}

#[derive(thiserror::Error, Debug)]
pub enum SubscribeError {
    #[error(transparent)]
    Group(#[from] Box<GroupError>),
    #[error(transparent)]
    NotFound(#[from] NotFound),
    // TODO: Add this to `NotFound`
    #[error("group message expected in database but is missing")]
    GroupMessageNotFound,
    #[error("processing group message in stream: {0}")]
    ReceiveGroup(#[from] Box<GroupMessageProcessingError>),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
    #[error(transparent)]
    MessageStream(#[from] stream_messages::MessageStreamError),
    #[error(transparent)]
    ConversationStream(#[from] stream_conversations::ConversationStreamError),
    #[error(transparent)]
    ApiClient(#[from] xmtp_api::ApiError),
    #[error("{0}")]
    BoxError(Box<dyn RetryableError + Send + Sync>),
    #[error(transparent)]
    Db(#[from] xmtp_db::ConnectionError),
}

impl From<GroupError> for SubscribeError {
    fn from(value: GroupError) -> Self {
        SubscribeError::Group(Box::new(value))
    }
}

impl From<GroupMessageProcessingError> for SubscribeError {
    fn from(value: GroupMessageProcessingError) -> Self {
        SubscribeError::ReceiveGroup(Box::new(value))
    }
}

impl RetryableError for SubscribeError {
    fn is_retryable(&self) -> bool {
        use SubscribeError::*;
        match self {
            Group(e) => retryable!(e),
            GroupMessageNotFound => true,
            ReceiveGroup(e) => retryable!(e),
            Storage(e) => retryable!(e),
            Decode(_) => false,
            NotFound(e) => retryable!(e),
            MessageStream(e) => retryable!(e),
            ConversationStream(e) => retryable!(e),
            ApiClient(e) => retryable!(e),
            BoxError(e) => retryable!(e),
            Db(c) => retryable!(c),
        }
    }
}

impl<Context> Client<Context>
where
    Context: XmtpSharedContext + Send + Sync + 'static,
{
    /// Async proxy for processing a streamed welcome message.
    /// Shouldn't be used unless for out-of-process utilities like Push Notifications.
    /// Pulls a new provider/database connection.
    pub async fn process_streamed_welcome_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<MlsGroup<Context>> {
        let conn = self.context.db();
        let envelope =
            WelcomeMessage::decode(envelope_bytes.as_slice()).map_err(SubscribeError::from)?;
        let known_welcomes = HashSet::from_iter(conn.group_welcome_ids()?.into_iter());
        let future = ProcessWelcomeFuture::new(
            known_welcomes,
            self.context.clone(),
            WelcomeOrGroup::Welcome(envelope),
            None,
        )?;
        match future.process().await? {
            ProcessWelcomeResult::New { group, .. } => Ok(group),
            ProcessWelcomeResult::NewStored { group, .. } => Ok(group),
            ProcessWelcomeResult::IgnoreId { .. } | ProcessWelcomeResult::Ignore => {
                Err(stream_conversations::ConversationStreamError::InvalidConversationType.into())
            }
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn stream_conversations(
        &self,
        conversation_type: Option<ConversationType>,
    ) -> Result<impl Stream<Item = Result<MlsGroup<Context>>> + use<'_, Context>>
    where
        Context::ApiClient: XmtpMlsStreams,
    {
        StreamConversations::new(&self.context, conversation_type).await
    }

    /// Stream conversations but decouple the lifetime of 'self' from the stream.
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn stream_conversations_owned(
        &self,
        conversation_type: Option<ConversationType>,
    ) -> Result<impl Stream<Item = Result<MlsGroup<Context>>> + 'static>
    where
        Context::ApiClient: XmtpMlsStreams,
    {
        StreamConversations::new_owned(self.context.clone(), conversation_type).await
    }
}

impl<Context> Client<Context>
where
    Context: XmtpSharedContext + Send + Sync + 'static,
    Context::ApiClient: XmtpMlsStreams + Send + Sync + 'static,
    Context::MlsStorage: Send + Sync + 'static,
{
    pub fn stream_conversations_with_callback(
        client: Arc<Client<Context>>,
        conversation_type: Option<ConversationType>,
        #[cfg(not(target_arch = "wasm32"))] mut convo_callback: impl FnMut(Result<MlsGroup<Context>>)
        + Send
        + 'static,
        #[cfg(target_arch = "wasm32")] mut convo_callback: impl FnMut(Result<MlsGroup<Context>>)
        + 'static,
        #[cfg(target_arch = "wasm32")] on_close: impl FnOnce() + 'static,
        #[cfg(not(target_arch = "wasm32"))] on_close: impl FnOnce() + Send + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>> {
        let (tx, rx) = oneshot::channel();

        xmtp_common::spawn(Some(rx), async move {
            let stream = client.stream_conversations(conversation_type).await?;
            futures::pin_mut!(stream);
            let _ = tx.send(());
            while let Some(convo) = stream.next().await {
                convo_callback(convo)
            }
            tracing::debug!("`stream_conversations` stream ended, dropping stream");
            on_close();
            Ok::<_, SubscribeError>(())
        })
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn stream_all_messages(
        &self,
        conversation_type: Option<ConversationType>,
        consent_state: Option<Vec<ConsentState>>,
    ) -> Result<impl Stream<Item = Result<StoredGroupMessage>> + '_> {
        tracing::debug!(
            inbox_id = self.inbox_id(),
            installation_id = %self.context.installation_id(),
            conversation_type = ?conversation_type,
            "stream all messages"
        );

        StreamAllMessages::new(&self.context, conversation_type, consent_state).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn stream_all_messages_owned(
        &self,
        conversation_type: Option<ConversationType>,
        consent_state: Option<Vec<ConsentState>>,
    ) -> Result<impl Stream<Item = Result<StoredGroupMessage>> + 'static> {
        tracing::debug!(
            inbox_id = self.inbox_id(),
            installation_id = %self.context.installation_id(),
            conversation_type = ?conversation_type,
            "stream all messages"
        );

        StreamAllMessages::new_owned(self.context.clone(), conversation_type, consent_state).await
    }

    pub fn stream_all_messages_with_callback(
        context: Context,
        conversation_type: Option<ConversationType>,
        consent_state: Option<Vec<ConsentState>>,
        #[cfg(not(target_arch = "wasm32"))] mut callback: impl FnMut(Result<StoredGroupMessage>)
        + Send
        + 'static,
        #[cfg(target_arch = "wasm32")] mut callback: impl FnMut(Result<StoredGroupMessage>) + 'static,
        #[cfg(target_arch = "wasm32")] on_close: impl FnOnce() + 'static,
        #[cfg(not(target_arch = "wasm32"))] on_close: impl FnOnce() + Send + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>> {
        let (tx, rx) = oneshot::channel();

        xmtp_common::spawn(Some(rx), async move {
            tracing::debug!("stream all messages with callback");
            let stream = StreamAllMessages::new(&context, conversation_type, consent_state).await?;

            futures::pin_mut!(stream);
            let _ = tx.send(());

            while let Some(message) = stream.next().await {
                callback(message)
            }
            tracing::debug!("`stream_all_messages` stream ended, dropping stream");
            on_close();
            Ok::<_, SubscribeError>(())
        })
    }

    pub fn stream_consent_with_callback(
        client: Arc<Client<Context>>,
        #[cfg(not(target_arch = "wasm32"))] mut callback: impl FnMut(Result<Vec<StoredConsentRecord>>)
        + Send
        + 'static,
        #[cfg(target_arch = "wasm32")] mut callback: impl FnMut(Result<Vec<StoredConsentRecord>>)
        + 'static,
        #[cfg(target_arch = "wasm32")] on_close: impl FnOnce() + 'static,
        #[cfg(not(target_arch = "wasm32"))] on_close: impl FnOnce() + Send + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>> {
        let (tx, rx) = oneshot::channel();

        xmtp_common::spawn(Some(rx), async move {
            let receiver = client.local_events.subscribe();
            let stream = receiver.stream_consent_updates();

            futures::pin_mut!(stream);
            let _ = tx.send(());
            while let Some(message) = stream.next().await {
                callback(message)
            }
            tracing::debug!("`stream_consent` stream ended, dropping stream");
            on_close();
            Ok::<_, SubscribeError>(())
        })
    }

    pub fn stream_preferences_with_callback(
        client: Arc<Client<Context>>,
        #[cfg(not(target_arch = "wasm32"))] mut callback: impl FnMut(Result<Vec<PreferenceUpdate>>)
        + Send
        + 'static,
        #[cfg(target_arch = "wasm32")] mut callback: impl FnMut(Result<Vec<PreferenceUpdate>>) + 'static,
        #[cfg(target_arch = "wasm32")] on_close: impl FnOnce() + 'static,
        #[cfg(not(target_arch = "wasm32"))] on_close: impl FnOnce() + Send + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>> {
        let (tx, rx) = oneshot::channel();

        xmtp_common::spawn(Some(rx), async move {
            let receiver = client.local_events.subscribe();
            let stream = receiver.stream_preference_updates();

            futures::pin_mut!(stream);
            let _ = tx.send(());
            while let Some(message) = stream.next().await {
                callback(message)
            }
            tracing::debug!("`stream_consent` stream ended, dropping stream");
            on_close();
            Ok::<_, SubscribeError>(())
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    /// A macro for asserting that a stream yields a specific decrypted message.
    ///
    /// # Example
    /// ```rust
    /// assert_msg!(stream, b"first");
    /// ```
    #[macro_export]
    macro_rules! assert_msg {
        ($stream:expr, $expected:expr) => {
            assert_eq!(
                String::from_utf8_lossy(
                    $stream
                        .next()
                        .await
                        .unwrap()
                        .unwrap()
                        .decrypted_message_bytes
                        .as_slice()
                ),
                String::from_utf8_lossy($expected.as_bytes())
            );
        };
    }

    /// A macro for asserting that a stream yields a specific decrypted message.
    ///
    /// # Example
    /// ```rust
    /// assert_msg!(stream, b"first");
    /// ```
    #[macro_export]
    macro_rules! assert_msg_exists {
        ($stream:expr) => {
            assert!(
                !$stream
                    .next()
                    .await
                    .unwrap()
                    .unwrap()
                    .decrypted_message_bytes
                    .is_empty()
            );
        };
    }
}
