use futures::{FutureExt, Stream, StreamExt};
use prost::Message;
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    pin::Pin,
    sync::Arc,
    task::Poll,
};
use tokio::{
    sync::{broadcast, oneshot},
    task::JoinHandle,
};
use tokio_stream::wrappers::BroadcastStream;
use tracing::instrument;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::{api_client::XmtpMlsStreams, xmtp::mls::api::v1::WelcomeMessage};

use stream_conversations::StreamConversations;
use stream_messages::{StreamGroupMessages, MessagesStreamInfo};

// mod stream_all;
mod stream_conversations;
pub(crate) mod stream_messages;

use crate::{
    client::{extract_welcome_message, ClientError},
    groups::{
        device_sync::preference_sync::UserPreferenceUpdate, mls_sync::GroupMessageProcessingError,
        GroupError, MlsGroup,
    },
    storage::{
        consent_record::StoredConsentRecord,
        group::{ConversationType, GroupQueryArgs, StoredGroup},
        group_message::StoredGroupMessage,
        ProviderTransactions, StorageError, NotFound
    },
    Client, XmtpApi,
};
use thiserror::Error;
use xmtp_common::{retryable, RetryableError};

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

// Wrappers to deal with Send Bounds
#[cfg(not(target_arch = "wasm32"))]
pub struct FutureWrapper<'a, O> {
    inner: Pin<Box<dyn Future<Output = O> + Send + 'a>>,
}

#[cfg(target_arch = "wasm32")]
pub struct FutureWrapper<'a, C> {
    inner: Pin<Box<dyn Future<Output = Result<(MlsGroup<C>, Option<i64>)>> + 'a>>,
}

impl<'a, O> Future for FutureWrapper<'a, O> {
    type Output = O;

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let inner = &mut self.inner;
        futures::pin_mut!(inner);
        inner.as_mut().poll(cx)
    }
}

impl<'a, O> FutureWrapper<'a, O> {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new<F>(future: F) -> Self
    where
        F: Future<Output = O> + Send + 'a,
    {
        Self {
            inner: future.boxed(),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new<F>(future: F) -> Self
    where
        F: Future<Output = O> + 'a,
    {
        Self {
            inner: future.boxed_local(),
        }
    }
}

#[derive(Debug)]
/// Wrapper around a [`tokio::task::JoinHandle`] but with a oneshot receiver
/// which allows waiting for a `with_callback` stream fn to be ready for stream items.
pub struct StreamHandle<T> {
    handle: JoinHandle<T>,
    start: Option<oneshot::Receiver<()>>,
}

/// Events local to this client
/// are broadcast across all senders/receivers of streams
#[derive(Clone)]
pub enum LocalEvents {
    // a new group was created
    NewGroup(Vec<u8>),
    SyncMessage(SyncMessage),
    OutgoingPreferenceUpdates(Vec<UserPreferenceUpdate>),
    IncomingPreferenceUpdate(Vec<UserPreferenceUpdate>),
}

#[derive(Clone)]
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
    fn stream_sync_messages(self) -> impl Stream<Item = Result<LocalEvents, SubscribeError>>;
    fn stream_consent_updates(
        self,
    ) -> impl Stream<Item = Result<Vec<StoredConsentRecord>, SubscribeError>>;
    fn stream_preference_updates(
        self,
    ) -> impl Stream<Item = Result<Vec<UserPreferenceUpdate>, SubscribeError>>;
}

impl StreamMessages for broadcast::Receiver<LocalEvents> {
    #[instrument(level = "trace", skip_all)]
    fn stream_sync_messages(self) -> impl Stream<Item = Result<LocalEvents, SubscribeError>> {
        BroadcastStream::new(self).filter_map(|event| async {
            xmtp_common::optify!(event, "Missed message due to event queue lag")
                .and_then(LocalEvents::sync_filter)
                .map(Result::Ok)
        })
    }

    fn stream_consent_updates(
        self,
    ) -> impl Stream<Item = Result<Vec<StoredConsentRecord>, SubscribeError>> {
        BroadcastStream::new(self).filter_map(|event| async {
            xmtp_common::optify!(event, "Missed message due to event queue lag")
                .and_then(LocalEvents::consent_filter)
                .map(Result::Ok)
        })
    }

    fn stream_preference_updates(
        self,
    ) -> impl Stream<Item = Result<Vec<UserPreferenceUpdate>, SubscribeError>> {
        BroadcastStream::new(self).filter_map(|event| async {
            xmtp_common::optify!(event, "Missed message due to event queue lag")
                .and_then(LocalEvents::preference_filter)
                .map(Result::Ok)
        })
    }
}

impl<T> StreamHandle<T> {
    /// Waits for the stream to be fully spawned
    pub async fn wait_for_ready(&mut self) {
        if let Some(s) = self.start.take() {
            let _ = s.await;
        }
    }
}

impl<T> From<StreamHandle<T>> for JoinHandle<T> {
    fn from(stream: StreamHandle<T>) -> JoinHandle<T> {
        stream.handle
    }
}

impl From<StoredGroup> for (Vec<u8>, MessagesStreamInfo) {
    fn from(group: StoredGroup) -> (Vec<u8>, MessagesStreamInfo) {
        (
            group.id,
            MessagesStreamInfo {
                cursor: 0,
            },
        )
    }
}

// TODO: REMOVE BEFORE MERGING
// TODO: REMOVE BEFORE MERGING
// TODO: REMOVE BEFORE MERGING
pub(self) mod temp {
    pub(super) type Result<T> = std::result::Result<T, super::SubscribeError>;
}

#[derive(thiserror::Error, Debug)]
pub enum SubscribeError {
    #[error(transparent)]
    Client(#[from] ClientError),
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
    Database(#[from] diesel::result::Error),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Api(#[from] xmtp_proto::Error),
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
    #[error(transparent)]
    MessageStream(#[from] stream_messages::MessageStreamError),
}

impl RetryableError for SubscribeError {
    fn is_retryable(&self) -> bool {
        use SubscribeError::*;
        match self {
            Client(e) => retryable!(e),
            Group(e) => retryable!(e),
            GroupMessageNotFound => true,
            ReceiveGroup(e) => retryable!(e),
            Database(e) => retryable!(e),
            Storage(e) => retryable!(e),
            Api(e) => retryable!(e),
            Decode(_) => false,
            NotFound(e) => retryable!(e),
            MessageStream(e) => retryable!(e),
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
    ) -> Result<MlsGroup<Self>, SubscribeError> {
        let provider = self.mls_provider()?;
        let conn = provider.conn_ref();
        let envelope = WelcomeMessage::decode(envelope_bytes.as_slice())
            .map_err(|e| ClientError::Generic(e.to_string()))?;
        let known_welcomes = HashSet::from_iter(conn.group_welcome_ids()?.into_iter());
        let (group, _) = StreamConversations::<_, ()>::on_welcome(
            &known_welcomes,
            self.clone(),
            &provider,
            envelope,
        )
        .await?;
        Ok(group)
    }

    // #[tracing::instrument(level = "debug", skip_all)]
    pub async fn stream_conversations<'a>(
        &'a self,
        conversation_type: Option<ConversationType>,
    ) -> Result<impl Stream<Item = Result<MlsGroup<Self>, SubscribeError>> + 'a, SubscribeError>
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
        mut convo_callback: impl FnMut(Result<MlsGroup<Self>, SubscribeError>) + Send + 'static,
    ) -> impl crate::StreamHandle<StreamOutput = Result<(), SubscribeError>> {
        let (tx, rx) = oneshot::channel();

        crate::spawn(Some(rx), async move {
            let stream = client.stream_conversations(conversation_type).await?;
            futures::pin_mut!(stream);
            let _ = tx.send(());
            while let Some(convo) = stream.next().await {
                tracing::info!("Trigger conversation callback");
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
    ) -> Result<impl Stream<Item = Result<StoredGroupMessage, SubscribeError>> + '_, SubscribeError>
    {
        tracing::debug!(
            inbox_id = self.inbox_id(),
            conversation_type = ?conversation_type,
            "stream all messages"
        );
        let mut group_list = async {
            let provider = self.mls_provider()?;
            self.sync_welcomes(&provider).await?;

            let group_list = provider
                .conn_ref()
                .find_groups(GroupQueryArgs::default().maybe_conversation_type(conversation_type))?
                .into_iter()
                .map(Into::into)
                .collect::<HashMap<Vec<u8>, MessagesStreamInfo>>();
            Ok::<_, SubscribeError>(group_list)
        }
        .await?;

        let stream = async_stream::stream! {
            let messages_stream = StreamGroupMessages::new(
                self,
                &group_list
            )
            .await?;
            futures::pin_mut!(messages_stream);

            let convo_stream = self.stream_conversations(conversation_type).await?;
            futures::pin_mut!(convo_stream);

            tracing::info!("\n\n Waiting on messages \n\n");
            let mut extra_messages = Vec::new();

            loop {
                tokio::select! {
                    // biased enforces an order to select!. If a message and a group are both ready
                    // at the same time, `biased` mode will process the message before the new
                    // group.
                    biased;

                    messages = futures::future::ready(&mut extra_messages), if !extra_messages.is_empty() => {
                        for message in messages.drain(0..) {
                            yield message;
                        }
                    },
                    Some(message) = messages_stream.next() => {
                        // an error can only mean the receiver has been dropped or closed so we're
                        // safe to end the stream
                        yield message;
                    }
                    Some(new_group) = convo_stream.next() => {
                        match new_group {
                            Ok(new_group) => {
                                tracing::info!("Received new conversation inside streamAllMessages");
                                if group_list.contains_key(&new_group.group_id) {
                                    continue;
                                }
                                for info in group_list.values_mut() {
                                    info.cursor = 0;
                                }
                                group_list.insert(
                                    new_group.group_id,
                                    MessagesStreamInfo {
                                        cursor: 1, // For the new group, stream all messages since the group was created
                                    },
                                );
                                let new_messages_stream = match StreamGroupMessages::new(self, &group_list).await {
                                    Ok(s) => s,
                                    Err(e) => {
                                        yield Err(e);
                                        continue;
                                    },
                                };

                                tracing::debug!("switching streams");
                                // attempt to drain all ready messages from existing stream
                                while let Some(Some(message)) = messages_stream.next().now_or_never() {
                                    extra_messages.push(message);
                                }
                                messages_stream.set(new_messages_stream);
                                continue;
                            },
                            Err(e) => {
                                yield Err(e)
                            }
                        }
                    },
                }
            }
        };

        Ok(stream)
    }

    pub fn stream_all_messages_with_callback(
        client: Arc<Client<ApiClient, V>>,
        conversation_type: Option<ConversationType>,
        mut callback: impl FnMut(Result<StoredGroupMessage, SubscribeError>) + Send + 'static,
    ) -> impl crate::StreamHandle<StreamOutput = Result<(), SubscribeError>> {
        let (tx, rx) = oneshot::channel();

        crate::spawn(Some(rx), async move {
            let stream = client.stream_all_messages(conversation_type).await?;
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
        mut callback: impl FnMut(Result<Vec<StoredConsentRecord>, SubscribeError>) + Send + 'static,
    ) -> impl crate::StreamHandle<StreamOutput = Result<(), ClientError>> {
        let (tx, rx) = oneshot::channel();

        crate::spawn(Some(rx), async move {
            let receiver = client.local_events.subscribe();
            let stream = receiver.stream_consent_updates();

            futures::pin_mut!(stream);
            let _ = tx.send(());
            while let Some(message) = stream.next().await {
                callback(message)
            }
            tracing::debug!("`stream_consent` stream ended, dropping stream");
            Ok::<_, ClientError>(())
        })
    }

    pub fn stream_preferences_with_callback(
        client: Arc<Client<ApiClient, V>>,
        mut callback: impl FnMut(Result<Vec<UserPreferenceUpdate>, SubscribeError>) + Send + 'static,
    ) -> impl crate::StreamHandle<StreamOutput = Result<(), ClientError>> {
        let (tx, rx) = oneshot::channel();

        crate::spawn(Some(rx), async move {
            let receiver = client.local_events.subscribe();
            let stream = receiver.stream_preference_updates();

            futures::pin_mut!(stream);
            let _ = tx.send(());
            while let Some(message) = stream.next().await {
                callback(message)
            }
            tracing::debug!("`stream_consent` stream ended, dropping stream");
            Ok::<_, ClientError>(())
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use crate::{
        builder::ClientBuilder,
        groups::GroupMetadataOptions,
        storage::{
            group::{ConversationType, GroupQueryArgs},
            group_message::StoredGroupMessage,
        },
        utils::test::{Delivery, TestClient},
        Client, StreamHandle,
    };
    use futures::StreamExt;
    use parking_lot::Mutex;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    };
    use wasm_bindgen_test::wasm_bindgen_test;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::InboxOwner;

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
                $stream
                    .next()
                    .await
                    .unwrap()
                    .unwrap()
                    .decrypted_message_bytes,
                $expected.as_bytes()
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

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread", worker_threads = 10))]
    async fn test_stream_all_messages_unchanging_group_list() {
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_group
            .add_members_by_inbox_id(&[caro.inbox_id()])
            .await
            .unwrap();

        let bo_group = bo
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        bo_group
            .add_members_by_inbox_id(&[caro.inbox_id()])
            .await
            .unwrap();

        let stream = caro.stream_all_messages(None).await.unwrap();
        futures::pin_mut!(stream);
        bo_group.send_message(b"first").await.unwrap();
        assert_msg!(stream, "first");

        bo_group.send_message(b"second").await.unwrap();
        assert_msg!(stream, "second");

        alix_group.send_message(b"third").await.unwrap();
        assert_msg!(stream, "third");

        bo_group.send_message(b"fourth").await.unwrap();
        assert_msg!(stream, "fourth");
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread", worker_threads = 10))]
    async fn test_stream_all_messages_changing_group_list() {
        let alix = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let caro_wallet = generate_local_wallet();
        let caro = Arc::new(ClientBuilder::new_test_client(&caro_wallet).await);

        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_group
            .add_members_by_inbox_id(&[caro.inbox_id()])
            .await
            .unwrap();

        let stream = caro.stream_all_messages(None).await.unwrap();
        futures::pin_mut!(stream);
        tracing::info!("\n\nSENDING FIRST MESSAGE\n\n");

        alix_group.send_message(b"first").await.unwrap();
        assert_msg!(stream, "first");

        let bo_group = bo.create_dm(caro_wallet.get_address()).await.unwrap();
        assert_msg_exists!(stream);

        bo_group.send_message(b"second").await.unwrap();
        assert_msg!(stream, "second");

        alix_group.send_message(b"third").await.unwrap();
        assert_msg!(stream, "third");

        let alix_group_2 = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_group_2
            .add_members_by_inbox_id(&[caro.inbox_id()])
            .await
            .unwrap();

        alix_group.send_message(b"fourth").await.unwrap();
        assert_msg!(stream, "fourth");

        alix_group_2.send_message(b"fifth").await.unwrap();
        assert_msg!(stream, "fifth");
    }

    #[ignore]
    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread"))]
    async fn test_stream_all_messages_does_not_lose_messages() {
        let alix = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let caro = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_group
            .add_members_by_inbox_id(&[caro.inbox_id()])
            .await
            .unwrap();

        let messages: Arc<Mutex<Vec<StoredGroupMessage>>> = Arc::new(Mutex::new(Vec::new()));
        let messages_clone = messages.clone();

        let blocked = Arc::new(AtomicU64::new(55));

        let blocked_pointer = blocked.clone();
        let mut handle = Client::<TestClient, _>::stream_all_messages_with_callback(
            caro.clone(),
            None,
            move |message| {
                (*messages_clone.lock()).push(message.unwrap());
                blocked_pointer.fetch_sub(1, Ordering::SeqCst);
            },
        );
        handle.wait_for_ready().await;

        let alix_group_pointer = alix_group.clone();
        crate::spawn(None, async move {
            for _ in 0..50 {
                alix_group_pointer.send_message(b"spam").await.unwrap();
                xmtp_common::time::sleep(core::time::Duration::from_micros(200)).await;
            }
        });

        for _ in 0..5 {
            let new_group = alix
                .create_group(None, GroupMetadataOptions::default())
                .unwrap();
            new_group
                .add_members_by_inbox_id(&[caro.inbox_id()])
                .await
                .unwrap();
            new_group
                .send_message(b"spam from new group")
                .await
                .unwrap();
        }

        let _ = tokio::time::timeout(core::time::Duration::from_secs(120), async {
            while blocked.load(Ordering::SeqCst) > 0 {
                tokio::task::yield_now().await;
            }
        })
        .await;

        let missed_messages = blocked.load(Ordering::SeqCst);
        if missed_messages > 0 {
            println!("Missed {} Messages", missed_messages);
            panic!("Test failed due to missed messages");
        }
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread"))]
    async fn test_self_group_creation() {
        let alix = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bo = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let groups = Arc::new(Mutex::new(Vec::new()));
        let notify = Delivery::new(None);
        let (notify_pointer, groups_pointer) = (notify.clone(), groups.clone());

        let closer = Client::<TestClient, _>::stream_conversations_with_callback(
            alix.clone(),
            Some(ConversationType::Group),
            move |g| {
                let mut groups = groups_pointer.lock();
                groups.push(g);
                notify_pointer.notify_one();
            },
        );

        alix.create_group(None, GroupMetadataOptions::default())
            .unwrap();

        notify
            .wait_for_delivery()
            .await
            .expect("Stream never received group");

        {
            let grps = groups.lock();
            assert_eq!(grps.len(), 1);
        }

        let group = bo
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&[alix.inbox_id()])
            .await
            .unwrap();

        notify.wait_for_delivery().await.unwrap();

        {
            let grps = groups.lock();
            assert_eq!(grps.len(), 2);
        }

        // Verify syncing welcomes while streaming causes no issues
        alix.sync_welcomes(&alix.mls_provider().unwrap())
            .await
            .unwrap();
        let find_groups_results = alix.find_groups(GroupQueryArgs::default()).unwrap();

        {
            let grps = groups.lock();
            assert_eq!(grps.len(), find_groups_results.len());
        }

        closer.end();
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread"))]
    #[cfg_attr(target_family = "wasm", ignore)]
    async fn test_dm_streaming() {
        let alix = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bo = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let stream = alix.stream_conversations(Some(ConversationType::Group)).await.unwrap();
        futures::pin_mut!(stream);

        alix.create_dm_by_inbox_id(bo.inbox_id().to_string())
            .await
            .unwrap();
        let result = xmtp_common::time::timeout(std::time::Duration::from_millis(100), stream.next()).await;
        assert!(result.is_err(), "Stream unexpectedly received a DM group");

        let group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();

        let group = stream.next().await.unwrap();
        assert!(group.is_ok());

        // Start a stream with only dms
        // Start a stream with conversation_type DM
        let stream = alix.stream_conversations(Some(ConversationType::Dm)).await.unwrap();
        futures::pin_mut!(stream);

        let group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();

        // we should not get a message
        let result = xmtp_common::time::timeout(std::time::Duration::from_millis(100), stream.next()).await;
        assert!(result.is_err(), "Stream unexpectedly received a Group");

        alix.create_dm_by_inbox_id(bo.inbox_id().to_string())
            .await
            .unwrap();
        let group = stream.next().await.unwrap();
        assert!(group.is_ok());

        // Start a stream with all conversations
        let mut groups = Vec::new();
        // Wait for 2 seconds for the group creation to be streamed
        let stream = alix.stream_conversations(None).await.unwrap();
futures::pin_mut!(stream);

        alix.create_dm_by_inbox_id(bo.inbox_id().to_string())
            .await
            .unwrap();
        let group = stream.next().await.unwrap();
        assert!(group.is_ok());
        groups.push(group.unwrap());

        let dm = bo
            .create_dm_by_inbox_id(alix.inbox_id().to_string())
            .await
            .unwrap();
        dm.add_members_by_inbox_id(&[alix.inbox_id()])
            .await
            .unwrap();
        let group = stream.next().await.unwrap();
        assert!(group.is_ok());
        groups.push(group.unwrap());

        let group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();
        let group = stream.next().await.unwrap();
        assert!(group.is_ok());
        groups.push(group.unwrap());

        assert_eq!(groups.len(), 3);
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread"))]
    async fn test_dm_stream_all_messages() {
        let alix = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bo = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();

        let alix_dm = alix
            .create_dm_by_inbox_id(bo.inbox_id().to_string())
            .await
            .unwrap();

        // Start a stream with only groups
        let messages: Arc<Mutex<Vec<StoredGroupMessage>>> = Arc::new(Mutex::new(Vec::new()));
        // Wait for 2 seconds for the group creation to be streamed
        let notify = Delivery::new(Some(1));
        let (notify_pointer, messages_pointer) = (notify.clone(), messages.clone());

        let mut closer = Client::<TestClient, _>::stream_all_messages_with_callback(
            bo.clone(),
            Some(ConversationType::Group),
            move |message| {
                let mut messages: parking_lot::lock_api::MutexGuard<
                    '_,
                    parking_lot::RawMutex,
                    Vec<StoredGroupMessage>,
                > = messages_pointer.lock();
                messages.push(message.unwrap());
                notify_pointer.notify_one();
            },
        );
        closer.wait_for_ready().await;

        alix_dm.send_message("first".as_bytes()).await.unwrap();

        let result = notify.wait_for_delivery().await;
        assert!(result.is_err(), "Stream unexpectedly received a DM message");

        alix_group.send_message("second".as_bytes()).await.unwrap();

        notify.wait_for_delivery().await.unwrap();
        {
            let msgs = messages.lock();
            assert_eq!(msgs.len(), 1);
        }

        closer.end();

        // Start a stream with only dms
        let messages: Arc<Mutex<Vec<StoredGroupMessage>>> = Arc::new(Mutex::new(Vec::new()));
        // Wait for 2 seconds for the group creation to be streamed
        let notify = Delivery::new(Some(1));
        let (notify_pointer, messages_pointer) = (notify.clone(), messages.clone());

        let mut closer = Client::<TestClient, _>::stream_all_messages_with_callback(
            bo.clone(),
            Some(ConversationType::Dm),
            move |message| {
                let mut messages: parking_lot::lock_api::MutexGuard<
                    '_,
                    parking_lot::RawMutex,
                    Vec<StoredGroupMessage>,
                > = messages_pointer.lock();
                messages.push(message.unwrap());
                notify_pointer.notify_one();
            },
        );
        closer.wait_for_ready().await;

        alix_group.send_message("first".as_bytes()).await.unwrap();

        let result = notify.wait_for_delivery().await;
        assert!(
            result.is_err(),
            "Stream unexpectedly received a Group message"
        );

        alix_dm.send_message("second".as_bytes()).await.unwrap();

        notify.wait_for_delivery().await.unwrap();
        {
            let msgs = messages.lock();
            assert_eq!(msgs.len(), 1);
        }

        closer.end();

        // Start a stream with all conversations
        let messages: Arc<Mutex<Vec<StoredGroupMessage>>> = Arc::new(Mutex::new(Vec::new()));
        // Wait for 2 seconds for the group creation to be streamed
        let notify = Delivery::new(Some(1));
        let (notify_pointer, messages_pointer) = (notify.clone(), messages.clone());

        let mut closer = Client::<TestClient, _>::stream_all_messages_with_callback(
            bo.clone(),
            None,
            move |message| {
                let mut messages = messages_pointer.lock();
                messages.push(message.unwrap());
                notify_pointer.notify_one();
            },
        );
        closer.wait_for_ready().await;

        alix_group.send_message("first".as_bytes()).await.unwrap();

        notify.wait_for_delivery().await.unwrap();
        {
            let msgs = messages.lock();
            assert_eq!(msgs.len(), 1);
        }

        alix_dm.send_message("second".as_bytes()).await.unwrap();

        notify.wait_for_delivery().await.unwrap();
        {
            let msgs = messages.lock();
            assert_eq!(msgs.len(), 2);
        }

        closer.end();
    }
}
