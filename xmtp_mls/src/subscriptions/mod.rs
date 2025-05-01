use futures::{Stream, StreamExt};
use prost::Message;
use std::{collections::HashSet, sync::Arc};
use tokio::sync::{broadcast, oneshot};
use tokio_stream::wrappers::BroadcastStream;
use tracing::instrument;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::{api_client::XmtpMlsStreams, xmtp::mls::api::v1::WelcomeMessage};

use stream_all::StreamAllMessages;
use stream_conversations::{ProcessWelcomeFuture, StreamConversations, WelcomeOrGroup};

mod stream_all;
mod stream_conversations;
pub(crate) mod stream_messages;

use crate::{
    groups::{
        device_sync::preference_sync::UserPreferenceUpdate, mls_sync::GroupMessageProcessingError,
        GroupError, MlsGroup,
    },
    Client, XmtpApi,
};
use thiserror::Error;
use xmtp_common::{retryable, RetryableError, StreamHandle};
use xmtp_db::{
    consent_record::{ConsentState, StoredConsentRecord},
    group::ConversationType,
    group_message::StoredGroupMessage,
    NotFound, StorageError,
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
    SyncMessage(SyncMessage),
    OutgoingPreferenceUpdates(Vec<UserPreferenceUpdate>),
    IncomingPreferenceUpdate(Vec<UserPreferenceUpdate>),
}

#[derive(Debug, Clone)]
pub enum SyncMessage {
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

    fn sync_filter(self) -> Option<Self> {
        use LocalEvents::*;

        match &self {
            SyncMessage(_) => Some(self),
            OutgoingPreferenceUpdates(_) => Some(self),
            IncomingPreferenceUpdate(_) => Some(self),
            _ => None,
        }
    }

    fn consent_filter(self) -> Option<Vec<StoredConsentRecord>> {
        use LocalEvents::*;

        match self {
            OutgoingPreferenceUpdates(updates) => {
                let updates = updates
                    .into_iter()
                    .filter_map(|pu| match pu {
                        UserPreferenceUpdate::ConsentUpdate(cr) => Some(cr),
                        _ => None,
                    })
                    .collect();
                Some(updates)
            }
            IncomingPreferenceUpdate(updates) => {
                let updates = updates
                    .into_iter()
                    .filter_map(|pu| match pu {
                        UserPreferenceUpdate::ConsentUpdate(cr) => Some(cr),
                        _ => None,
                    })
                    .collect();
                Some(updates)
            }
            _ => None,
        }
    }

    fn preference_filter(self) -> Option<Vec<UserPreferenceUpdate>> {
        use LocalEvents::*;

        match self {
            OutgoingPreferenceUpdates(updates) => {
                let updates = updates
                    .into_iter()
                    .filter_map(|pu| match pu {
                        UserPreferenceUpdate::ConsentUpdate(_) => None,
                        _ => Some(pu),
                    })
                    .collect();
                Some(updates)
            }
            IncomingPreferenceUpdate(updates) => {
                let updates = updates
                    .into_iter()
                    .filter_map(|pu| match pu {
                        UserPreferenceUpdate::ConsentUpdate(_) => None,
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
    fn stream_sync_messages(self) -> impl Stream<Item = Result<LocalEvents>>;
    fn stream_consent_updates(self) -> impl Stream<Item = Result<Vec<StoredConsentRecord>>>;
    fn stream_preference_updates(self) -> impl Stream<Item = Result<Vec<UserPreferenceUpdate>>>;
}

impl StreamMessages for broadcast::Receiver<LocalEvents> {
    #[instrument(level = "debug", skip_all)]
    fn stream_sync_messages(self) -> impl Stream<Item = Result<LocalEvents>> {
        BroadcastStream::new(self).filter_map(|event| async {
            xmtp_common::optify!(event, "Missed message due to event queue lag")
                .and_then(LocalEvents::sync_filter)
                .map(Result::Ok)
        })
    }

    #[instrument(level = "debug", skip_all)]
    fn stream_consent_updates(self) -> impl Stream<Item = Result<Vec<StoredConsentRecord>>> {
        BroadcastStream::new(self).filter_map(|event| async {
            xmtp_common::optify!(event, "Missed message due to event queue lag")
                .and_then(LocalEvents::consent_filter)
                .map(Result::Ok)
        })
    }

    #[instrument(level = "debug", skip_all)]
    fn stream_preference_updates(self) -> impl Stream<Item = Result<Vec<UserPreferenceUpdate>>> {
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
    Group(#[from] GroupError),
    #[error(transparent)]
    NotFound(#[from] NotFound),
    // TODO: Add this to `NotFound`
    #[error("group message expected in database but is missing")]
    GroupMessageNotFound,
    #[error("processing group message in stream: {0}")]
    ReceiveGroup(#[from] GroupMessageProcessingError),
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
        }
    }
}

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi + Send + Sync + 'static,
    V: SmartContractSignatureVerifier + Send + Sync + 'static,
{
    /// Async proxy for processing a streamed welcome message.
    /// Shouldn't be used unless for out-of-process utilities like Push Notifications.
    /// Pulls a new provider/database connection.
    pub async fn process_streamed_welcome_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<MlsGroup<Self>> {
        let provider = self.mls_provider()?;
        let conn = provider.conn_ref();
        let envelope =
            WelcomeMessage::decode(envelope_bytes.as_slice()).map_err(SubscribeError::from)?;
        let known_welcomes = HashSet::from_iter(conn.group_welcome_ids()?.into_iter());
        let future = ProcessWelcomeFuture::new(
            known_welcomes,
            self.clone(),
            WelcomeOrGroup::Welcome(envelope),
            None,
        )?;
        future
            .process()
            .await?
            .map(|(group, _)| group)
            .ok_or_else(|| {
                stream_conversations::ConversationStreamError::InvalidConversationType.into()
            })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn stream_conversations(
        &self,
        conversation_type: Option<ConversationType>,
    ) -> Result<impl Stream<Item = Result<MlsGroup<Self>>> + use<'_, ApiClient, V>>
    where
        ApiClient: XmtpMlsStreams,
    {
        StreamConversations::new(self, conversation_type).await
    }
}

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi + XmtpMlsStreams + Send + Sync + 'static,
    V: SmartContractSignatureVerifier + Send + Sync + 'static,
{
    pub fn stream_conversations_with_callback(
        client: Arc<Client<ApiClient, V>>,
        conversation_type: Option<ConversationType>,
        #[cfg(not(target_arch = "wasm32"))] mut convo_callback: impl FnMut(Result<MlsGroup<Self>>)
            + Send
            + 'static,
        #[cfg(target_arch = "wasm32")] mut convo_callback: impl FnMut(Result<MlsGroup<Self>>) + 'static,
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
            Ok::<_, SubscribeError>(())
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn stream_all_messages(
        &self,
        conversation_type: Option<ConversationType>,
        consent_state: Option<Vec<ConsentState>>,
    ) -> Result<impl Stream<Item = Result<StoredGroupMessage>> + '_> {
        tracing::debug!(
            inbox_id = self.inbox_id(),
            installation_id = %self.context().installation_public_key(),
            conversation_type = ?conversation_type,
            "stream all messages"
        );

        StreamAllMessages::new(self, conversation_type, consent_state).await
    }

    pub fn stream_all_messages_with_callback(
        client: Arc<Client<ApiClient, V>>,
        conversation_type: Option<ConversationType>,
        consent_state: Option<Vec<ConsentState>>,
        #[cfg(not(target_arch = "wasm32"))] mut callback: impl FnMut(Result<StoredGroupMessage>)
            + Send
            + 'static,
        #[cfg(target_arch = "wasm32")] mut callback: impl FnMut(Result<StoredGroupMessage>) + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>> {
        let (tx, rx) = oneshot::channel();

        xmtp_common::spawn(Some(rx), async move {
            let stream = client
                .stream_all_messages(conversation_type, consent_state)
                .await?;
            futures::pin_mut!(stream);
            let _ = tx.send(());
            while let Some(message) = stream.next().await {
                callback(message)
            }
            tracing::debug!("`stream_all_messages` stream ended, dropping stream");
            Ok::<_, SubscribeError>(())
        })
    }

    pub fn stream_consent_with_callback(
        client: Arc<Client<ApiClient, V>>,
        #[cfg(not(target_arch = "wasm32"))] mut callback: impl FnMut(Result<Vec<StoredConsentRecord>>)
            + Send
            + 'static,
        #[cfg(target_arch = "wasm32")] mut callback: impl FnMut(Result<Vec<StoredConsentRecord>>)
            + 'static,
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
            Ok::<_, SubscribeError>(())
        })
    }

    pub fn stream_preferences_with_callback(
        client: Arc<Client<ApiClient, V>>,
        #[cfg(not(target_arch = "wasm32"))] mut callback: impl FnMut(Result<Vec<UserPreferenceUpdate>>)
            + Send
            + 'static,
        #[cfg(target_arch = "wasm32")] mut callback: impl FnMut(Result<Vec<UserPreferenceUpdate>>)
            + 'static,
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
            assert!(!$stream
                .next()
                .await
                .unwrap()
                .unwrap()
                .decrypted_message_bytes
                .is_empty());
        };
    }
}
