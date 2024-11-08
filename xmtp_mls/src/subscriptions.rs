use futures::{FutureExt, Stream, StreamExt};
use prost::Message;
use std::{collections::HashMap, sync::Arc};
use tokio::{
    sync::{broadcast, oneshot},
    task::JoinHandle,
};
use tokio_stream::wrappers::BroadcastStream;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::{api_client::XmtpMlsStreams, xmtp::mls::api::v1::WelcomeMessage};

use crate::{
    client::{extract_welcome_message, ClientError, MessageProcessingError},
    groups::{group_metadata::ConversationType, subscriptions, GroupError, MlsGroup},
    retry::{Retry, RetryableError},
    retry_async, retryable,
    storage::{
        group::{GroupQueryArgs, StoredGroup},
        group_message::StoredGroupMessage,
        StorageError,
    },
    Client, XmtpApi,
};

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
pub enum LocalEvents<C> {
    // a new group was created
    NewGroup(MlsGroup<C>),
    SyncMessage(SyncMessage),
}

#[derive(Clone)]
pub enum SyncMessage {
    Request { message_id: Vec<u8> },
    Reply { message_id: Vec<u8> },
}

impl<C> LocalEvents<C> {
    fn group_filter(self) -> Option<MlsGroup<C>> {
        use LocalEvents::*;
        // this is just to protect against any future variants
        match self {
            NewGroup(c) => Some(c),
            _ => None,
        }
    }

    pub(crate) fn sync_filter(self) -> Option<SyncMessage> {
        use LocalEvents::*;
        match self {
            SyncMessage(msg) => Some(msg),
            _ => None,
        }
    }
}

pub(crate) trait StreamMessages {
    fn stream_sync_messages(self) -> impl Stream<Item = Result<SyncMessage, SubscribeError>>;
}

impl<C> StreamMessages for broadcast::Receiver<LocalEvents<C>>
where
    C: Clone + Send + Sync + 'static,
{
    fn stream_sync_messages(self) -> impl Stream<Item = Result<SyncMessage, SubscribeError>> {
        BroadcastStream::new(self).filter_map(|event| async {
            crate::optify!(event, "Missed message due to event queue lag")
                .and_then(LocalEvents::sync_filter)
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

#[derive(Clone, Debug)]
pub(crate) struct MessagesStreamInfo {
    pub convo_created_at_ns: i64,
    pub cursor: u64,
}

impl From<StoredGroup> for (Vec<u8>, MessagesStreamInfo) {
    fn from(group: StoredGroup) -> (Vec<u8>, MessagesStreamInfo) {
        (
            group.id,
            MessagesStreamInfo {
                convo_created_at_ns: group.created_at_ns,
                cursor: 0,
            },
        )
    }
}

#[derive(thiserror::Error, Debug)]
pub enum SubscribeError {
    #[error("failed to start new messages stream {0}")]
    FailedToStartNewMessagesStream(ClientError),
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error(transparent)]
    Group(#[from] GroupError),
    #[error("group message expected in database but is missing")]
    GroupMessageNotFound,
    #[error("processing message in stream: {0}")]
    Receive(#[from] MessageProcessingError),
    #[error(transparent)]
    Database(#[from] diesel::result::Error),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Api(#[from] xmtp_proto::api_client::Error),
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
}

impl RetryableError for SubscribeError {
    fn is_retryable(&self) -> bool {
        use SubscribeError::*;
        match self {
            FailedToStartNewMessagesStream(e) => retryable!(e),
            Client(e) => retryable!(e),
            Group(e) => retryable!(e),
            GroupMessageNotFound => true,
            Receive(e) => retryable!(e),
            Database(e) => retryable!(e),
            Storage(e) => retryable!(e),
            Api(e) => retryable!(e),
            Decode(_) => false,
        }
    }
}

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi + Send + Sync + 'static,
    V: SmartContractSignatureVerifier + Send + Sync + 'static,
{
    async fn process_streamed_welcome(
        &self,
        welcome: WelcomeMessage,
    ) -> Result<MlsGroup<Self>, ClientError> {
        let welcome_v1 = extract_welcome_message(welcome)?;
        let creation_result = retry_async!(
            Retry::default(),
            (async {
                tracing::info!(
                    installation_id = &welcome_v1.id,
                    "Trying to process streamed welcome"
                );
                let welcome_v1 = &welcome_v1;
                self.context
                    .store()
                    .transaction_async(|provider| async move {
                        MlsGroup::create_from_encrypted_welcome(
                            Arc::new(self.clone()),
                            &provider,
                            welcome_v1.hpke_public_key.as_slice(),
                            &welcome_v1.data,
                            welcome_v1.id as i64,
                        )
                        .await
                    })
                    .await
            })
        );

        if let Some(err) = creation_result.as_ref().err() {
            let conn = self.context.store().conn()?;
            let result = conn.find_group_by_welcome_id(welcome_v1.id as i64);
            match result {
                Ok(Some(group)) => {
                    tracing::info!(
                        group_id = hex::encode(&group.id),
                        welcome_id = ?group.welcome_id,
                        "Loading existing group for welcome_id: {:?}",
                        group.welcome_id
                    );
                    return Ok(MlsGroup::new(self.clone(), group.id, group.created_at_ns));
                }
                Ok(None) => return Err(ClientError::Generic(err.to_string())),
                Err(e) => return Err(ClientError::Generic(e.to_string())),
            }
        }

        Ok(creation_result?)
    }

    pub async fn process_streamed_welcome_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<MlsGroup<Self>, ClientError> {
        let envelope = WelcomeMessage::decode(envelope_bytes.as_slice())
            .map_err(|e| ClientError::Generic(e.to_string()))?;

        let welcome = self.process_streamed_welcome(envelope).await?;
        Ok(welcome)
    }

    pub async fn stream_conversations(
        &self,
        conversation_type: Option<ConversationType>,
    ) -> Result<impl Stream<Item = Result<MlsGroup<Self>, SubscribeError>> + '_, ClientError>
    where
        ApiClient: XmtpMlsStreams,
    {
        let event_queue = tokio_stream::wrappers::BroadcastStream::new(
            self.local_events.subscribe(),
        )
        .filter_map(|event| async {
            crate::optify!(event, "Missed messages due to event queue lag")
                .and_then(LocalEvents::group_filter)
                .map(Result::Ok)
        });

        // Helper function for filtering Dm groups
        let filter_group = move |group: Result<MlsGroup<Self>, ClientError>| {
            let conversation_type = &conversation_type;
            // take care of any possible errors
            let result = || -> Result<_, _> {
                let group = group?;
                let provider = group.client.context().mls_provider()?;
                let metadata = group.metadata(&provider)?;
                Ok((metadata, group))
            };
            let filtered = result().map(|(metadata, group)| {
                conversation_type
                    .map_or(true, |ct| ct == metadata.conversation_type)
                    .then_some(group)
            });
            futures::future::ready(filtered.transpose())
        };

        let installation_key = self.installation_public_key();
        let id_cursor = 0;

        tracing::info!("Setting up conversation stream");
        let subscription = self
            .api_client
            .subscribe_welcome_messages(installation_key, Some(id_cursor))
            .await?;

        let stream = subscription
            .map(|welcome| async {
                tracing::info!("Received conversation streaming payload");
                self.process_streamed_welcome(welcome?).await
            })
            .filter_map(|v| async { Some(v.await) });

        Ok(futures::stream::select(stream, event_queue).filter_map(filter_group))
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
    ) -> impl crate::StreamHandle<StreamOutput = Result<(), ClientError>> {
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
            Ok::<_, ClientError>(())
        })
    }

    pub async fn stream_all_messages(
        &self,
        conversation_type: Option<ConversationType>,
    ) -> Result<impl Stream<Item = Result<StoredGroupMessage, SubscribeError>> + '_, ClientError>
    {
        let conn = self.store().conn()?;
        self.sync_welcomes(&conn).await?;

        let mut group_id_to_info = self
            .store()
            .conn()?
            .find_groups(GroupQueryArgs::default().maybe_conversation_type(conversation_type))?
            .into_iter()
            .map(Into::into)
            .collect::<HashMap<Vec<u8>, MessagesStreamInfo>>();

        let stream = async_stream::stream! {
            let messages_stream = subscriptions::stream_messages(
                self,
                Arc::new(group_id_to_info.clone())
            )
            .await?;
            futures::pin_mut!(messages_stream);

            tracing::info!("Setting up conversation stream in stream_all_messages");
            let convo_stream = self.stream_conversations(conversation_type).await?;

            futures::pin_mut!(convo_stream);

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
                                if group_id_to_info.contains_key(&new_group.group_id) {
                                    continue;
                                }
                                for info in group_id_to_info.values_mut() {
                                    info.cursor = 0;
                                }
                                group_id_to_info.insert(
                                    new_group.group_id,
                                    MessagesStreamInfo {
                                        convo_created_at_ns: new_group.created_at_ns,
                                        cursor: 1, // For the new group, stream all messages since the group was created
                                    },
                                );
                                let new_messages_stream = match subscriptions::stream_messages(
                                    self,
                                    Arc::new(group_id_to_info.clone())
                                ).await {
                                    Ok(s) => s,
                                    Err(e) => {
                                        yield Err(SubscribeError::FailedToStartNewMessagesStream(e));
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
    ) -> impl crate::StreamHandle<StreamOutput = Result<(), ClientError>> {
        let (tx, rx) = oneshot::channel();

        crate::spawn(Some(rx), async move {
            let stream = client.stream_all_messages(conversation_type).await?;
            futures::pin_mut!(stream);
            let _ = tx.send(());
            while let Some(message) = stream.next().await {
                callback(message)
            }
            tracing::debug!("`stream_all_messages` stream ended, dropping stream");
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
        groups::{group_metadata::ConversationType, GroupMetadataOptions},
        storage::{group::GroupQueryArgs, group_message::StoredGroupMessage},
        utils::test::{Delivery, FullXmtpClient, TestClient},
        Client, StreamHandle,
    };
    use futures::StreamExt;
    use parking_lot::Mutex;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    };
    use xmtp_cryptography::utils::generate_local_wallet;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test(flavor = "current_thread"))]
    async fn test_stream_welcomes() {
        let alice = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bob = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let alice_bob_group = alice
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        // FIXME:insipx we run into an issue where the reqwest::post().send() request
        // blocks the executor and we cannot progress the runtime if we dont `tokio::spawn` this.
        // A solution might be to use `hyper` instead, and implement a custom connection pool with
        // `deadpool`. This is a bit more work but shouldn't be too complicated since
        // we're only using `post` requests. It would be nice for all streams to work
        // w/o spawning a separate task.
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
        let bob_ptr = bob.clone();
        crate::spawn(None, async move {
            let bob_stream = bob_ptr.stream_conversations(None).await.unwrap();
            futures::pin_mut!(bob_stream);
            while let Some(item) = bob_stream.next().await {
                let _ = tx.send(item);
            }
        });

        let group_id = alice_bob_group.group_id.clone();
        alice_bob_group
            .add_members_by_inbox_id(&[bob.inbox_id()])
            .await
            .unwrap();

        let bob_received_groups = stream.next().await.unwrap().unwrap();
        assert_eq!(bob_received_groups.group_id, group_id);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 10)
    )]
    async fn test_stream_messages() {
        let alice = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bob = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alice_group = alice
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        // let mut bob_stream = bob.stream_conversations().await.unwrap()warning: unused implementer of `futures::Future` that must be used;
        alice_group
            .add_members_by_inbox_id(&[bob.inbox_id()])
            .await
            .unwrap();
        let bob_group = bob
            .sync_welcomes(&bob.store().conn().unwrap())
            .await
            .unwrap();
        let bob_group = bob_group.first().unwrap();

        let notify = Delivery::new(None);
        let notify_ptr = notify.clone();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        crate::spawn(None, async move {
            let stream = alice_group.stream().await.unwrap();
            futures::pin_mut!(stream);
            while let Some(item) = stream.next().await {
                let _ = tx.send(item);
                notify_ptr.notify_one();
            }
        });
        let mut stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);

        bob_group.send_message(b"hello").await.unwrap();
        notify.wait_for_delivery().await.unwrap();
        let message = stream.next().await.unwrap().unwrap();
        assert_eq!(message.decrypted_message_bytes, b"hello");

        bob_group.send_message(b"hello2").await.unwrap();
        notify.wait_for_delivery().await.unwrap();
        let message = stream.next().await.unwrap().unwrap();
        assert_eq!(message.decrypted_message_bytes, b"hello2");

        // assert_eq!(bob_received_groups.group_id, alice_bob_group.group_id);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 10)
    )]
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
        crate::sleep(core::time::Duration::from_millis(100)).await;

        let messages: Arc<Mutex<Vec<StoredGroupMessage>>> = Arc::new(Mutex::new(Vec::new()));
        let messages_clone = messages.clone();

        let notify = Delivery::new(None);
        let notify_pointer = notify.clone();
        let mut handle = Client::<TestClient, _>::stream_all_messages_with_callback(
            Arc::new(caro),
            None,
            move |message| {
                (*messages_clone.lock()).push(message.unwrap());
                notify_pointer.notify_one();
            },
        );
        handle.wait_for_ready().await;

        alix_group.send_message("first".as_bytes()).await.unwrap();
        notify
            .wait_for_delivery()
            .await
            .expect("didn't get `first`");
        bo_group.send_message("second".as_bytes()).await.unwrap();
        notify.wait_for_delivery().await.unwrap();
        alix_group.send_message("third".as_bytes()).await.unwrap();
        notify.wait_for_delivery().await.unwrap();
        bo_group.send_message("fourth".as_bytes()).await.unwrap();
        notify.wait_for_delivery().await.unwrap();

        let messages = messages.lock();
        assert_eq!(messages[0].decrypted_message_bytes, b"first");
        assert_eq!(messages[1].decrypted_message_bytes, b"second");
        assert_eq!(messages[2].decrypted_message_bytes, b"third");
        assert_eq!(messages[3].decrypted_message_bytes, b"fourth");
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 10)
    )]
    async fn test_stream_all_messages_changing_group_list() {
        let alix = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;
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
        let delivery = Delivery::new(None);
        let delivery_pointer = delivery.clone();
        let mut handle = Client::<TestClient, _>::stream_all_messages_with_callback(
            caro.clone(),
            None,
            move |message| {
                delivery_pointer.notify_one();
                (*messages_clone.lock()).push(message.unwrap());
            },
        );
        handle.wait_for_ready().await;

        alix_group.send_message("first".as_bytes()).await.unwrap();
        delivery
            .wait_for_delivery()
            .await
            .expect("timed out waiting for `first`");

        let bo_group = bo
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        bo_group
            .add_members_by_inbox_id(&[caro.inbox_id()])
            .await
            .unwrap();

        bo_group.send_message("second".as_bytes()).await.unwrap();
        delivery
            .wait_for_delivery()
            .await
            .expect("timed out waiting for `second`");

        alix_group.send_message("third".as_bytes()).await.unwrap();
        delivery
            .wait_for_delivery()
            .await
            .expect("timed out waiting for `third`");

        let alix_group_2 = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_group_2
            .add_members_by_inbox_id(&[caro.inbox_id()])
            .await
            .unwrap();

        alix_group.send_message("fourth".as_bytes()).await.unwrap();
        delivery
            .wait_for_delivery()
            .await
            .expect("timed out waiting for `fourth`");

        alix_group_2.send_message("fifth".as_bytes()).await.unwrap();
        delivery
            .wait_for_delivery()
            .await
            .expect("timed out waiting for `fifth`");

        {
            let messages = messages.lock();
            assert_eq!(messages.len(), 5);
        }

        let a = handle.abort_handle();
        a.end();
        let _ = handle.join().await;
        assert!(a.is_finished());

        alix_group
            .send_message("should not show up".as_bytes())
            .await
            .unwrap();
        crate::sleep(core::time::Duration::from_millis(100)).await;

        let messages = messages.lock();
        assert_eq!(messages.len(), 5);
    }

    #[ignore]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 10)
    )]
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
                crate::sleep(core::time::Duration::from_micros(200)).await;
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

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test(flavor = "multi_thread"))]
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
        alix.sync_welcomes(&alix.store().conn().unwrap())
            .await
            .unwrap();
        let find_groups_results = alix.find_groups(GroupQueryArgs::default()).unwrap();

        {
            let grps = groups.lock();
            assert_eq!(grps.len(), find_groups_results.len());
        }

        closer.end();
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test(flavor = "multi_thread"))]
    async fn test_dm_streaming() {
        let alix = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bo = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let groups = Arc::new(Mutex::new(Vec::new()));
        // Wait for 2 seconds for the group creation to be streamed
        let notify = Delivery::new(Some(1));
        let (notify_pointer, groups_pointer) = (notify.clone(), groups.clone());

        // Start a stream with enableDm set to false
        let mut closer = Client::<TestClient, _>::stream_conversations_with_callback(
            alix.clone(),
            Some(ConversationType::Group),
            move |g| {
                let mut groups = groups_pointer.lock();
                groups.push(g);
                notify_pointer.notify_one();
            },
        );

        alix.create_dm_by_inbox_id(bo.inbox_id().to_string())
            .await
            .unwrap();

        let result = notify.wait_for_delivery().await;
        assert!(result.is_err(), "Stream unexpectedly received a DM group");

        let group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();

        notify.wait_for_delivery().await.unwrap();
        {
            let grps = groups.lock();
            assert_eq!(grps.len(), 1);
        }

        let _ = closer.end_and_wait().await;

        // Start a stream with only dms
        let groups = Arc::new(Mutex::new(Vec::new()));
        let notify = Delivery::new(Some(1));
        let (notify_pointer, groups_pointer) = (notify.clone(), groups.clone());

        // Start a stream with conversation_type DM
        let closer = Client::<TestClient, _>::stream_conversations_with_callback(
            alix.clone(),
            Some(ConversationType::Dm),
            move |g| {
                let mut groups = groups_pointer.lock();
                groups.push(g);
                notify_pointer.notify_one();
            },
        );

        let group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();

        let result = notify.wait_for_delivery().await;
        assert!(result.is_err(), "Stream unexpectedly received a Group");

        alix.create_dm_by_inbox_id(bo.inbox_id().to_string())
            .await
            .unwrap();
        notify.wait_for_delivery().await.unwrap();
        {
            let grps = groups.lock();
            assert_eq!(grps.len(), 1);
        }

        closer.end();

        // Start a stream with all conversations
        let groups = Arc::new(Mutex::new(Vec::new()));
        // Wait for 2 seconds for the group creation to be streamed
        let notify = Delivery::new(None);
        let (notify_pointer, groups_pointer) = (notify.clone(), groups.clone());
        let closer =
            FullXmtpClient::stream_conversations_with_callback(alix.clone(), None, move |g| {
                let mut groups = groups_pointer.lock();
                groups.push(g);
                notify_pointer.notify_one();
            });

        alix.create_dm_by_inbox_id(bo.inbox_id().to_string())
            .await
            .unwrap();
        notify.wait_for_delivery().await.unwrap();
        {
            let grps = groups.lock();
            assert_eq!(grps.len(), 1);
        }

        let dm = bo
            .create_dm_by_inbox_id(alix.inbox_id().to_string())
            .await
            .unwrap();
        dm.add_members_by_inbox_id(&[alix.inbox_id()])
            .await
            .unwrap();
        notify.wait_for_delivery().await.unwrap();
        {
            let grps = groups.lock();
            assert_eq!(grps.len(), 2);
        }

        let group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();

        notify.wait_for_delivery().await.unwrap();
        {
            let grps = groups.lock();
            assert_eq!(grps.len(), 3);
        }

        closer.end();
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test(flavor = "multi_thread"))]
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
