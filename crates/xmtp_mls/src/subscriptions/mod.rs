use futures::{FutureExt, Stream, StreamExt, TryStreamExt, future, stream as future_stream};
use process_welcome::ProcessWelcomeFuture;
use std::{collections::HashSet, sync::Arc};
use tokio::sync::{broadcast, oneshot};
use tokio_stream::wrappers::BroadcastStream;
use xmtp_api_d14n::protocol::{EnvelopeError, V3WelcomeMessageExtractor, WelcomeMessageExtractor};
use xmtp_api_d14n::stream;
use xmtp_proto::types::WelcomeMessage;

use tracing::instrument;
use xmtp_db::prelude::*;
use xmtp_proto::api_client::XmtpMlsStreams;

use process_welcome::ProcessWelcomeResult;
use stream_all::StreamAllMessages;
use stream_conversations::{StreamConversations, WelcomeOrGroup};

pub(crate) mod d14n_compat;
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
    messages::decoded_message::DecodedMessage,
    subscriptions::d14n_compat::{V3OrD14n, decode_welcome_message},
};
use thiserror::Error;
use xmtp_common::{ErrorCode, MaybeSend, RetryableError, StreamHandle, retryable};
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
    // a message was deleted (contains the decoded message that was deleted)
    MessageDeleted(Box<DecodedMessage>),
}

#[derive(Clone)]
pub enum SyncWorkerEvent {
    NewSyncGroupFromWelcome(Vec<u8>),
    NewSyncGroupMsg,
    // The sync worker will auto-sync these with other devices.
    SyncPreferences(Vec<PreferenceUpdate>),
    CycleHMAC,
    Tick,
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
            Self::Tick => write!(f, "Tick"),
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
            Self::PreferencesChanged(updates) => Some(updates),
            _ => None,
        }
    }

    fn message_deletion_filter(self) -> Option<Box<DecodedMessage>> {
        match self {
            Self::MessageDeleted(message) => Some(message),
            _ => None,
        }
    }
}

pub(crate) trait StreamMessages {
    fn stream_consent_updates(self) -> impl Stream<Item = Result<Vec<StoredConsentRecord>>>;
    fn stream_preference_updates(self) -> impl Stream<Item = Result<Vec<PreferenceUpdate>>>;
    fn stream_message_deletions(self) -> impl Stream<Item = Result<Box<DecodedMessage>>>;
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
    fn stream_message_deletions(self) -> impl Stream<Item = Result<Box<DecodedMessage>>> {
        BroadcastStream::new(self).filter_map(|event| async {
            xmtp_common::optify!(event, "Missed message due to event queue lag")
                .and_then(LocalEvents::message_deletion_filter)
                .map(Result::Ok)
        })
    }
}

#[derive(thiserror::Error, Debug, ErrorCode)]
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
}

impl SubscribeError {
    pub fn dyn_err(other: impl RetryableError + 'static) -> Self {
        SubscribeError::BoxError(Box::new(other) as _)
    }
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
    ) -> Result<Vec<MlsGroup<Context>>> {
        let conn = self.context.db();
        let mut known_welcomes = HashSet::from_iter(conn.group_cursors()?.into_iter());
        let welcome = decode_welcome_message(envelope_bytes.as_slice())?;
        let welcomes: Vec<_> = match welcome {
            V3OrD14n::D14n(s) => {
                let messages = s.envelopes;
                stream::try_extractor::<_, WelcomeMessageExtractor>(future_stream::once(
                    future::ready(Ok::<_, EnvelopeError>(messages)),
                ))
                .try_collect()
                .now_or_never()
                .expect("stream has no pending operations, created with one item")
            }
            V3OrD14n::V3(message) => {
                let s: Vec<WelcomeMessage> = stream::try_extractor::<_, V3WelcomeMessageExtractor>(
                    future_stream::iter(vec![Ok::<_, EnvelopeError>(vec![message])]),
                )
                .try_collect::<Vec<WelcomeMessage>>()
                .now_or_never()
                .expect("stream must not fail because it is statically created with one item")?
                .into_iter()
                .collect();
                Ok(s)
            }
        }?;

        let mut out = Vec::with_capacity(welcomes.len());
        for welcome in welcomes {
            let welcome_id = welcome.cursor;
            let future = ProcessWelcomeFuture::new(
                known_welcomes.clone(),
                self.context.clone(),
                WelcomeOrGroup::Welcome(welcome),
                None,
                false,
                None,
            )?;

            match future.process().await? {
                ProcessWelcomeResult::New { group, .. } => {
                    known_welcomes.insert(welcome_id);
                    out.push(group)
                }
                ProcessWelcomeResult::NewStored { group, .. } => {
                    known_welcomes.insert(welcome_id);
                    out.push(group)
                }
                ProcessWelcomeResult::IgnoreId { .. } | ProcessWelcomeResult::Ignore => {
                    known_welcomes.insert(welcome_id);
                    return Err(
                        stream_conversations::ConversationStreamError::InvalidConversationType
                            .into(),
                    );
                }
            }
        }
        Ok(out)
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
        mut callback: impl FnMut(Result<DecodedMessage>) + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>> {
        let (tx, rx) = oneshot::channel();

        xmtp_common::spawn(Some(rx), async move {
            let receiver = client.local_events.subscribe();
            let stream = receiver.stream_message_deletions();

            futures::pin_mut!(stream);
            let _ = tx.send(());
            while let Some(message) = stream.next().await {
                callback(message.map(|boxed| *boxed))
            }
            tracing::debug!("`stream_message_deletions` stream ended, dropping stream");
            Ok::<_, SubscribeError>(())
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::context::XmtpSharedContext;
    use crate::tester;
    use xmtp_api_d14n::protocol::XmtpQuery;

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

    #[cfg(not(feature = "d14n"))]
    #[xmtp_common::test(flavor = "multi_thread", worker_threads = 5, unwrap_try = true)]
    async fn test_process_streamed_welcome_message_v3() {
        use prost::Message;

        tester!(alix);
        tester!(bo);

        // Alix creates a group and adds Bo
        let alix_group = alix.create_group(None, None)?;
        alix_group.add_members(&[bo.inbox_id()]).await?;

        // Query the welcome message envelope using query_at
        let envelope = alix
            .context
            .api()
            .query_at(
                xmtp_proto::types::TopicKind::WelcomeMessagesV1
                    .create(bo.context.installation_id()),
                None,
            )
            .await?;

        // Get the welcome messages and encode the first one as V3 protobuf
        let welcomes = envelope.welcome_messages()?;
        assert!(!welcomes.is_empty(), "Should have at least one welcome");

        let welcome = &welcomes[0];
        let v1 = welcome.as_v1().expect("Should be a V1 welcome");

        // Manually construct the protobuf welcome message from V1 fields
        let mut envelope_bytes = Vec::new();
        let proto_welcome = xmtp_proto::xmtp::mls::api::v1::WelcomeMessage {
            version: Some(
                xmtp_proto::xmtp::mls::api::v1::welcome_message::Version::V1(
                    xmtp_proto::xmtp::mls::api::v1::welcome_message::V1 {
                        id: welcome.sequence_id(),
                        created_ns: welcome.timestamp() as u64,
                        installation_key: v1.installation_key.to_vec(),
                        data: v1.data.clone(),
                        hpke_public_key: v1.hpke_public_key.clone(),
                        wrapper_algorithm: v1.wrapper_algorithm as i32,
                        welcome_metadata: v1.welcome_metadata.clone(),
                    },
                ),
            ),
        };
        proto_welcome.encode(&mut envelope_bytes)?;

        // Process the streamed welcome message
        let groups = bo.process_streamed_welcome_message(envelope_bytes).await?;

        assert_eq!(groups.len(), 1, "Should have exactly one group");
    }

    #[cfg(feature = "d14n")]
    #[xmtp_common::test(flavor = "multi_thread", worker_threads = 5, unwrap_try = true)]
    async fn test_process_streamed_welcome_message_d14n() {
        use prost::Message;
        use xmtp_proto::types::TopicKind;

        tester!(alix);
        tester!(bo);

        // Alix creates a group and adds Bo
        let alix_group = alix.create_group(None, None)?;
        alix_group.add_members(&[bo.inbox_id()]).await?;

        // Query the welcome envelope using query_at for D14n format
        let envelope = alix
            .context
            .api()
            .query_at(
                TopicKind::WelcomeMessagesV1.create(bo.context.installation_id()),
                None,
            )
            .await?;

        // Get the client envelopes and cursors
        let client_envelopes = envelope.client_envelopes()?;
        let cursors = envelope.cursors()?;

        // Wrap in D14n envelope structure
        let mut envelope_bytes = Vec::new();
        xmtp_proto::xmtp::xmtpv4::message_api::SubscribeEnvelopesResponse {
            envelopes: client_envelopes
                .into_iter()
                .zip(cursors.iter())
                .map(|(client_env, cursor)| {
                    use xmtp_proto::xmtp::xmtpv4::envelopes::*;

                    let mut client_bytes = Vec::new();
                    client_env.encode(&mut client_bytes).unwrap();

                    let payer_envelope = PayerEnvelope {
                        unsigned_client_envelope: client_bytes,
                        payer_signature: None,
                        target_originator: cursor.originator_id,
                        message_retention_days: 30,
                    };

                    let mut payer_bytes = Vec::new();
                    payer_envelope.encode(&mut payer_bytes).unwrap();

                    let unsigned_originator_envelope = UnsignedOriginatorEnvelope {
                        originator_node_id: cursor.originator_id,
                        originator_sequence_id: cursor.sequence_id,
                        originator_ns: 1000000,
                        payer_envelope_bytes: payer_bytes,
                        base_fee_picodollars: 0,
                        congestion_fee_picodollars: 0,
                        expiry_unixtime: 0,
                    };

                    let mut unsigned_bytes = Vec::new();
                    unsigned_originator_envelope
                        .encode(&mut unsigned_bytes)
                        .unwrap();

                    OriginatorEnvelope {
                        unsigned_originator_envelope: unsigned_bytes,
                        proof: Some(originator_envelope::Proof::OriginatorSignature(
                            Default::default(),
                        )),
                    }
                })
                .collect(),
        }
        .encode(&mut envelope_bytes)?;

        // Process the streamed welcome message
        let groups = bo.process_streamed_welcome_message(envelope_bytes).await?;

        assert_eq!(groups.len(), 1, "Should have exactly one group");
    }
}
