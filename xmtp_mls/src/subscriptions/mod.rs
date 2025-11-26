use futures::{Stream, StreamExt};
use process_welcome::ProcessWelcomeFuture;
use prost::Message;
use std::{collections::HashSet, sync::Arc};
use tokio::sync::{broadcast, oneshot};
use tokio_stream::wrappers::BroadcastStream;
use xmtp_api_d14n::protocol::{Extractor, ProtocolEnvelope as _};

use tracing::instrument;
use xmtp_db::prelude::*;
use xmtp_proto::{api_client::XmtpMlsStreams, xmtp::mls::api::v1::WelcomeMessage};

use process_welcome::ProcessWelcomeResult;
use stream_all::StreamAllMessages;
use stream_conversations::{StreamConversations, WelcomeOrGroup};

pub mod process_message;
pub mod process_welcome;
mod stream_all;
mod stream_conversations;
pub mod stream_messages;
mod stream_utils;

#[cfg(any(test, feature = "test-utils"))]
use crate::subscriptions::stream_messages::stream_stats::{StreamStatsWrapper, StreamWithStats};

use crate::{
    Client,
    context::XmtpSharedContext,
    groups::{
        GroupError, MlsGroup, device_sync::preference_sync::PreferenceUpdate,
        mls_sync::GroupMessageProcessingError,
    },
};
use thiserror::Error;
use xmtp_common::{MaybeSend, RetryableError, StreamHandle, retryable};
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
#[derive(Debug, Clone, PartialEq)]
pub enum LocalEvents {
    // a new group was created
    NewGroup(Vec<u8>),
    PreferencesChanged(Vec<PreferenceUpdate>),
    // a message was deleted (contains message ID)
    MessageDeleted(Vec<u8>),
}

#[derive(Clone)]
pub enum SyncWorkerEvent {
    NewSyncGroupFromWelcome(Vec<u8>),
    NewSyncGroupMsg,
    // The sync worker will auto-sync these with other devices.
    SyncPreferences(Vec<PreferenceUpdate>),
    CycleHMAC,
}

impl std::fmt::Debug for SyncWorkerEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NewSyncGroupFromWelcome(arg0) => f
                .debug_tuple("NewSyncGroupFromWelcome")
                .field(&hex::encode(arg0))
                .finish(),
            Self::NewSyncGroupMsg => write!(f, "NewSyncGroupMsg"),
            Self::SyncPreferences(arg0) => f.debug_tuple("SyncPreferences").field(arg0).finish(),
            Self::CycleHMAC => write!(f, "CycleHMAC"),
        }
    }
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

    fn message_deletion_filter(self) -> Option<Vec<u8>> {
        match self {
            Self::MessageDeleted(message_id) => Some(message_id),
            _ => None,
        }
    }
}

pub(crate) trait StreamMessages {
    fn stream_consent_updates(self) -> impl Stream<Item = Result<Vec<StoredConsentRecord>>>;
    fn stream_preference_updates(self) -> impl Stream<Item = Result<Vec<PreferenceUpdate>>>;
    fn stream_message_deletions(self) -> impl Stream<Item = Result<Vec<u8>>>;
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

    #[instrument(level = "trace", skip_all)]
    fn stream_message_deletions(self) -> impl Stream<Item = Result<Vec<u8>>> {
        BroadcastStream::new(self).filter_map(|event| async {
            xmtp_common::optify!(event, "Missed message due to event queue lag")
                .and_then(LocalEvents::message_deletion_filter)
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
    BoxError(Box<dyn RetryableError>),
    #[error(transparent)]
    Db(#[from] xmtp_db::ConnectionError),
    #[error(transparent)]
    Conversion(#[from] xmtp_proto::ConversionError),
    #[error(transparent)]
    Envelope(#[from] xmtp_api_d14n::protocol::EnvelopeError),
    #[error("the originators of the messages do not match expected: {expected}, got: {got}")]
    MismatchedOriginators { expected: u32, got: u32 },
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
            Conversion(c) => retryable!(c),
            Envelope(c) => retryable!(c),
            // this is an error which should never occur
            MismatchedOriginators { .. } => false,
        }
    }
}

impl<Context> Client<Context>
where
    Context: XmtpSharedContext + 'static,
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
        let known_welcomes = HashSet::from_iter(conn.group_cursors()?.into_iter());
        let mut extractor = xmtp_api_d14n::protocol::V3WelcomeMessageExtractor::default();
        envelope.accept(&mut extractor)?;
        let welcome: xmtp_proto::types::WelcomeMessage = extractor.get()?;

        let future = ProcessWelcomeFuture::new(
            known_welcomes,
            self.context.clone(),
            WelcomeOrGroup::Welcome(welcome),
            None,
            false,
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
        include_duplicate_dms: bool,
    ) -> Result<impl Stream<Item = Result<MlsGroup<Context>>> + use<'_, Context>>
    where
        Context::ApiClient: XmtpMlsStreams,
    {
        StreamConversations::new(
            &self.context,
            conversation_type,
            include_duplicate_dms,
            None,
        )
        .await
    }

    /// Stream conversations but decouple the lifetime of 'self' from the stream.
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn stream_conversations_owned(
        &self,
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
    ) -> Result<impl Stream<Item = Result<MlsGroup<Context>>> + 'static>
    where
        Context::ApiClient: XmtpMlsStreams,
    {
        StreamConversations::new_owned(
            self.context.clone(),
            conversation_type,
            include_duplicate_dms,
            None,
        )
        .await
    }
}

impl<Context> Client<Context>
where
    Context: XmtpSharedContext + 'static,
    Context::ApiClient: XmtpMlsStreams + 'static,
    Context::MlsStorage: 'static,
{
    pub fn stream_conversations_with_callback(
        client: Arc<Client<Context>>,
        conversation_type: Option<ConversationType>,
        mut convo_callback: impl FnMut(Result<MlsGroup<Context>>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
        include_duplicate_dms: bool,
    ) -> impl StreamHandle<StreamOutput = Result<()>> {
        let (tx, rx) = oneshot::channel();

        xmtp_common::spawn(Some(rx), async move {
            let stream = match client
                .stream_conversations(conversation_type, include_duplicate_dms)
                .await
            {
                Ok(stream) => stream,
                Err(e) => {
                    tracing::warn!("Failed to create conversation stream, closing: {}", e);
                    on_close();
                    return Ok::<_, SubscribeError>(());
                }
            };
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

    #[tracing::instrument(level = "trace", skip_all)]
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn stream_all_messages_owned_with_stats(
        &self,
        conversation_type: Option<ConversationType>,
        consent_state: Option<Vec<ConsentState>>,
    ) -> Result<impl StreamWithStats<Item = Result<StoredGroupMessage>> + 'static> {
        tracing::debug!(
            inbox_id = self.inbox_id(),
            installation_id = %self.context.installation_id(),
            conversation_type = ?conversation_type,
            "stream all messages"
        );

        let stream =
            StreamAllMessages::new_owned(self.context.clone(), conversation_type, consent_state)
                .await?;

        Ok(StreamStatsWrapper::new(stream))
    }

    pub fn stream_all_messages_with_callback(
        context: Context,
        conversation_type: Option<ConversationType>,
        consent_state: Option<Vec<ConsentState>>,
        mut callback: impl FnMut(Result<StoredGroupMessage>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>> {
        let (tx, rx) = oneshot::channel();

        xmtp_common::spawn(Some(rx), async move {
            tracing::debug!("stream all messages with callback");
            let stream =
                match StreamAllMessages::new(&context, conversation_type, consent_state).await {
                    Ok(stream) => stream,
                    Err(e) => {
                        tracing::warn!("Failed to create message stream, closing: {}", e);
                        on_close();
                        return Ok::<_, SubscribeError>(());
                    }
                };

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
        mut callback: impl FnMut(Result<Vec<StoredConsentRecord>>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
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
        mut callback: impl FnMut(Result<Vec<PreferenceUpdate>>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
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
            tracing::debug!("`stream_preferences` stream ended, dropping stream");
            on_close();
            Ok::<_, SubscribeError>(())
        })
    }

    pub fn stream_message_deletions_with_callback(
        client: Arc<Client<Context>>,
        mut callback: impl FnMut(Result<Vec<u8>>) + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>> {
        let (tx, rx) = oneshot::channel();

        xmtp_common::spawn(Some(rx), async move {
            let receiver = client.local_events.subscribe();
            let stream = receiver.stream_message_deletions();

            futures::pin_mut!(stream);
            let _ = tx.send(());
            while let Some(message_id) = stream.next().await {
                callback(message_id)
            }
            tracing::debug!("`stream_message_deletions` stream ended, dropping stream");
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
