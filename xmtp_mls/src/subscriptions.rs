use std::{collections::HashMap, pin::Pin, sync::Arc};

use futures::{FutureExt, Stream, StreamExt};
use prost::Message;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, UnboundedReceiverStream};
use xmtp_proto::xmtp::mls::api::v1::WelcomeMessage;

use crate::{
    api::GroupFilter,
    client::{extract_welcome_message, ClientError},
    groups::{extract_group_id, GroupError, MlsGroup},
    retry::Retry,
    retry_async,
    storage::{group::StoredGroup, group_message::StoredGroupMessage},
    Client, XmtpApi,
};

#[derive(Debug)]
/// Wrapper around a [`tokio::task::JoinHandle`] but with a oneshot receiver
/// which allows waiting for a `with_callback` stream fn to be ready for stream items.
pub struct StreamHandle<T> {
    pub handle: JoinHandle<T>,
    start: Option<oneshot::Receiver<()>>,
}

/// Events local to this client
/// are broadcast across all senders/receivers of streams
#[derive(Clone, Debug)]
pub(crate) enum LocalEvents {
    // a new group was created
    NewGroup(MlsGroup),
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

impl<ApiClient> Client<ApiClient>
where
    ApiClient: XmtpApi,
{
    async fn process_streamed_welcome(
        &self,
        welcome: WelcomeMessage,
    ) -> Result<MlsGroup, ClientError> {
        let welcome_v1 = extract_welcome_message(welcome)?;
        let creation_result = retry_async!(
            Retry::default(),
            (async {
                let welcome_v1 = welcome_v1.clone();
                self.context
                    .store
                    .transaction_async(|provider| async move {
                        MlsGroup::create_from_encrypted_welcome(
                            self,
                            &provider,
                            welcome_v1.hpke_public_key.as_slice(),
                            welcome_v1.data,
                            welcome_v1.id as i64,
                        )
                        .await
                    })
                    .await
            })
        );

        if let Some(err) = creation_result.as_ref().err() {
            let conn = self.context.store.conn()?;
            let result = conn.find_group_by_welcome_id(welcome_v1.id as i64);
            match result {
                Ok(Some(group)) => {
                    log::info!(
                        "Loading existing group for welcome_id: {:?}",
                        group.welcome_id
                    );
                    return Ok(MlsGroup::new(
                        self.context.clone(),
                        group.id,
                        group.created_at_ns,
                    ));
                }
                Ok(None) => return Err(ClientError::Generic(err.to_string())),
                Err(e) => return Err(ClientError::Generic(e.to_string())),
            }
        }

        Ok(creation_result.unwrap())
    }

    pub async fn process_streamed_welcome_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<MlsGroup, ClientError> {
        let envelope = WelcomeMessage::decode(envelope_bytes.as_slice())
            .map_err(|e| ClientError::Generic(e.to_string()))?;

        let welcome = self.process_streamed_welcome(envelope).await?;
        Ok(welcome)
    }

    pub async fn stream_conversations(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = MlsGroup> + Send + '_>>, ClientError> {
        let event_queue =
            tokio_stream::wrappers::BroadcastStream::new(self.local_events.subscribe());

        let event_queue = event_queue.filter_map(|event| async move {
            match event {
                Ok(LocalEvents::NewGroup(g)) => Some(g),
                Err(BroadcastStreamRecvError::Lagged(missed)) => {
                    log::warn!("Missed {missed} messages due to local event queue lagging");
                    None
                }
            }
        });

        let installation_key = self.installation_public_key();
        let id_cursor = 0;

        let subscription = self
            .api_client
            .subscribe_welcome_messages(installation_key, Some(id_cursor))
            .await?;

        let stream = subscription
            .map(|welcome| async {
                log::info!("Received conversation streaming payload");
                self.process_streamed_welcome(welcome?).await
            })
            .filter_map(|res| async {
                match res.await {
                    Ok(group) => Some(group),
                    Err(err) => {
                        log::error!("Error processing stream entry for conversation: {:?}", err);
                        None
                    }
                }
            });

        Ok(Box::pin(futures::stream::select(stream, event_queue)))
    }

    #[tracing::instrument(skip(self, group_id_to_info))]
    pub(crate) async fn stream_messages(
        self: Arc<Self>,
        group_id_to_info: HashMap<Vec<u8>, MessagesStreamInfo>,
    ) -> Result<Pin<Box<dyn Stream<Item = StoredGroupMessage> + Send>>, ClientError> {
        let filters: Vec<GroupFilter> = group_id_to_info
            .iter()
            .map(|(group_id, info)| GroupFilter::new(group_id.clone(), Some(info.cursor)))
            .collect();
        let messages_subscription = self.api_client.subscribe_group_messages(filters).await?;

        let stream = messages_subscription
            .map(move |res| {
                let context = self.context.clone();
                let client = self.clone();

                let group_id_to_info = group_id_to_info.clone();
                async move {
                    match res {
                        Ok(envelope) => {
                            log::info!("Received message streaming payload");
                            let group_id = extract_group_id(&envelope)?;
                            let stream_info = group_id_to_info.get(&group_id).ok_or(
                                ClientError::StreamInconsistency(
                                    "Received message for a non-subscribed group".to_string(),
                                ),
                            )?;
                            let mls_group =
                                MlsGroup::new(context, group_id, stream_info.convo_created_at_ns);

                            mls_group
                                .process_stream_entry(envelope.clone(), client.clone())
                                .await
                        }
                        Err(err) => Err(GroupError::Api(err)),
                    }
                }
            })
            .filter_map(move |res| async {
                match res.await {
                    Ok(Some(message)) => Some(message),
                    Ok(None) => {
                        log::info!("Skipped message streaming payload");
                        None
                    }
                    Err(err) => {
                        log::error!("Error processing stream entry: {:?}", err);
                        None
                    }
                }
            });

        Ok(Box::pin(stream))
    }
}

impl<ApiClient> Client<ApiClient>
where
    ApiClient: XmtpApi,
{
    pub fn stream_conversations_with_callback(
        client: Arc<Client<ApiClient>>,
        mut convo_callback: impl FnMut(MlsGroup) + Send + 'static,
    ) -> StreamHandle<Result<(), ClientError>> {
        let (tx, rx) = oneshot::channel();

        let handle = tokio::spawn(async move {
            let mut stream = client.stream_conversations().await.unwrap();
            let _ = tx.send(());
            while let Some(convo) = stream.next().await {
                convo_callback(convo)
            }
            Ok(())
        });

        StreamHandle {
            start: Some(rx),
            handle,
        }
    }

    pub(crate) fn stream_messages_with_callback(
        client: Arc<Client<ApiClient>>,
        group_id_to_info: HashMap<Vec<u8>, MessagesStreamInfo>,
        mut callback: impl FnMut(StoredGroupMessage) + Send + 'static,
    ) -> StreamHandle<Result<(), ClientError>> {
        let (tx, rx) = oneshot::channel();

        let handle = tokio::spawn(async move {
            let mut stream = Self::stream_messages(client, group_id_to_info).await?;
            let _ = tx.send(());
            while let Some(message) = stream.next().await {
                callback(message)
            }
            Ok(())
        });

        StreamHandle {
            start: Some(rx),
            handle,
        }
    }

    pub async fn stream_all_messages(
        client: Arc<Client<ApiClient>>,
    ) -> Result<impl Stream<Item = StoredGroupMessage>, ClientError> {
        let (tx, rx) = mpsc::unbounded_channel();

        client.sync_welcomes().await?;

        let mut group_id_to_info = client
            .store()
            .conn()?
            .find_groups(None, None, None, None)?
            .into_iter()
            .map(Into::into)
            .collect::<HashMap<Vec<u8>, MessagesStreamInfo>>();

        tokio::spawn(async move {
            let client = client.clone();
            let mut messages_stream = client
                .clone()
                .stream_messages(group_id_to_info.clone())
                .await?;
            let mut convo_stream = Self::stream_conversations(&client).await?;
            let mut extra_messages = Vec::new();

            loop {
                tokio::select! {
                    // biased enforces an order to select!. If a message and a group are both ready
                    // at the same time, `biased` mode will process the message before the new
                    // group.
                    biased;

                    messages = futures::future::ready(&mut extra_messages), if !extra_messages.is_empty() => {
                        for message in messages.drain(0..) {
                            if tx.send(message).is_err() {
                                break;
                            }
                        }
                    },
                    Some(message) = messages_stream.next() => {
                        // an error can only mean the receiver has been dropped or closed so we're
                        // safe to end the stream
                        if tx.send(message).is_err() {
                            break;
                        }
                    }
                    Some(new_group) = convo_stream.next() => {
                        if tx.is_closed() {
                            break;
                        }
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
                        let new_messages_stream = client.clone().stream_messages(group_id_to_info.clone()).await?;

                        // attempt to drain all ready messages from existing stream
                        while let Some(Some(message)) = messages_stream.next().now_or_never() {
                            extra_messages.push(message);
                        }
                        let _ = std::mem::replace(&mut messages_stream, new_messages_stream);
                    },
                }
            }
            Ok::<_, ClientError>(())
        });

        Ok(UnboundedReceiverStream::new(rx))
    }

    pub fn stream_all_messages_with_callback(
        client: Arc<Client<ApiClient>>,
        mut callback: impl FnMut(StoredGroupMessage) + Send + Sync + 'static,
    ) -> StreamHandle<Result<(), ClientError>> {
        let (tx, rx) = oneshot::channel();

        let handle = tokio::spawn(async move {
            let mut stream = Self::stream_all_messages(client).await?;
            let _ = tx.send(());
            while let Some(message) = stream.next().await {
                callback(message)
            }
            Ok(())
        });

        StreamHandle {
            start: Some(rx),
            handle,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::test::Delivery;
    use crate::{
        builder::ClientBuilder, groups::GroupMetadataOptions,
        storage::group_message::StoredGroupMessage, Client,
    };
    use futures::StreamExt;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    };
    use xmtp_api_grpc::grpc_api_helper::Client as GrpcClient;
    use xmtp_cryptography::utils::generate_local_wallet;

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_stream_welcomes() {
        let alice = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bob = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alice_bob_group = alice
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        let mut bob_stream = bob.stream_conversations().await.unwrap();
        alice_bob_group
            .add_members_by_inbox_id(&alice, vec![bob.inbox_id()])
            .await
            .unwrap();

        let bob_received_groups = bob_stream.next().await.unwrap();
        assert_eq!(bob_received_groups.group_id, alice_bob_group.group_id);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_stream_all_messages_unchanging_group_list() {
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_group
            .add_members_by_inbox_id(&alix, vec![caro.inbox_id()])
            .await
            .unwrap();

        let bo_group = bo
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        bo_group
            .add_members_by_inbox_id(&bo, vec![caro.inbox_id()])
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let messages: Arc<Mutex<Vec<StoredGroupMessage>>> = Arc::new(Mutex::new(Vec::new()));
        let messages_clone = messages.clone();

        let notify = Delivery::new();
        let notify_pointer = notify.clone();
        let mut handle = Client::<GrpcClient>::stream_all_messages_with_callback(
            Arc::new(caro),
            move |message| {
                (*messages_clone.lock().unwrap()).push(message);
                notify_pointer.notify_one();
            },
        );
        handle.wait_for_ready().await;

        alix_group
            .send_message("first".as_bytes(), &alix)
            .await
            .unwrap();
        notify.wait_for_delivery().await.unwrap();
        bo_group
            .send_message("second".as_bytes(), &bo)
            .await
            .unwrap();
        notify.wait_for_delivery().await.unwrap();
        alix_group
            .send_message("third".as_bytes(), &alix)
            .await
            .unwrap();
        notify.wait_for_delivery().await.unwrap();
        bo_group
            .send_message("fourth".as_bytes(), &bo)
            .await
            .unwrap();
        notify.wait_for_delivery().await.unwrap();

        let messages = messages.lock().unwrap();
        assert_eq!(messages[0].decrypted_message_bytes, b"first");
        assert_eq!(messages[1].decrypted_message_bytes, b"second");
        assert_eq!(messages[2].decrypted_message_bytes, b"third");
        assert_eq!(messages[3].decrypted_message_bytes, b"fourth");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_stream_all_messages_changing_group_list() {
        let alix = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let caro = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_group
            .add_members_by_inbox_id(&alix, vec![caro.inbox_id()])
            .await
            .unwrap();

        let messages: Arc<Mutex<Vec<StoredGroupMessage>>> = Arc::new(Mutex::new(Vec::new()));
        let messages_clone = messages.clone();
        let delivery = Delivery::new();
        let delivery_pointer = delivery.clone();
        let mut handle =
            Client::<GrpcClient>::stream_all_messages_with_callback(caro.clone(), move |message| {
                delivery_pointer.notify_one();
                (*messages_clone.lock().unwrap()).push(message);
            });
        handle.wait_for_ready().await;

        alix_group
            .send_message("first".as_bytes(), &alix)
            .await
            .unwrap();
        delivery.wait_for_delivery().await.unwrap();

        let bo_group = bo
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        bo_group
            .add_members_by_inbox_id(&bo, vec![caro.inbox_id()])
            .await
            .unwrap();

        bo_group
            .send_message("second".as_bytes(), &bo)
            .await
            .unwrap();
        delivery.wait_for_delivery().await.unwrap();

        alix_group
            .send_message("third".as_bytes(), &alix)
            .await
            .unwrap();
        delivery.wait_for_delivery().await.unwrap();

        let alix_group_2 = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_group_2
            .add_members_by_inbox_id(&alix, vec![caro.inbox_id()])
            .await
            .unwrap();

        alix_group
            .send_message("fourth".as_bytes(), &alix)
            .await
            .unwrap();
        delivery.wait_for_delivery().await.unwrap();

        alix_group_2
            .send_message("fifth".as_bytes(), &alix)
            .await
            .unwrap();
        delivery.wait_for_delivery().await.unwrap();

        {
            let messages = messages.lock().unwrap();
            assert_eq!(messages.len(), 5);
        }

        let a = handle.handle.abort_handle();
        a.abort();
        let _ = handle.handle.await;
        assert!(a.is_finished());

        alix_group
            .send_message("should not show up".as_bytes(), &alix)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let messages = messages.lock().unwrap();
        assert_eq!(messages.len(), 5);
    }

    #[ignore]
    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_stream_all_messages_does_not_lose_messages() {
        let alix = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let caro = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_group
            .add_members_by_inbox_id(&alix, vec![caro.inbox_id()])
            .await
            .unwrap();

        let messages: Arc<Mutex<Vec<StoredGroupMessage>>> = Arc::new(Mutex::new(Vec::new()));
        let messages_clone = messages.clone();

        let blocked = Arc::new(AtomicU64::new(55));

        let blocked_pointer = blocked.clone();
        let mut handle =
            Client::<GrpcClient>::stream_all_messages_with_callback(caro.clone(), move |message| {
                (*messages_clone.lock().unwrap()).push(message);
                blocked_pointer.fetch_sub(1, Ordering::SeqCst);
            });
        handle.wait_for_ready().await;

        let alix_group_pointer = alix_group.clone();
        let alix_pointer = alix.clone();
        tokio::spawn(async move {
            for _ in 0..50 {
                alix_group_pointer
                    .send_message(b"spam", &alix_pointer)
                    .await
                    .unwrap();
                tokio::time::sleep(std::time::Duration::from_micros(200)).await;
            }
        });

        for _ in 0..5 {
            let new_group = alix
                .create_group(None, GroupMetadataOptions::default())
                .unwrap();
            new_group
                .add_members_by_inbox_id(&alix, vec![caro.inbox_id()])
                .await
                .unwrap();
            new_group
                .send_message(b"spam from new group", &alix)
                .await
                .unwrap();
        }

        let _ = tokio::time::timeout(std::time::Duration::from_secs(120), async {
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_self_group_creation() {
        let alix = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bo = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let groups = Arc::new(Mutex::new(Vec::new()));
        let notify = Delivery::new();
        let (notify_pointer, groups_pointer) = (notify.clone(), groups.clone());

        let closer =
            Client::<GrpcClient>::stream_conversations_with_callback(alix.clone(), move |g| {
                let mut groups = groups_pointer.lock().unwrap();
                groups.push(g);
                notify_pointer.notify_one();
            });

        alix.create_group(None, GroupMetadataOptions::default())
            .unwrap();

        notify
            .wait_for_delivery()
            .await
            .expect("Stream never received group");

        {
            let grps = groups.lock().unwrap();
            assert_eq!(grps.len(), 1);
        }

        let group = bo
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&bo, vec![alix.inbox_id()])
            .await
            .unwrap();

        notify.wait_for_delivery().await.unwrap();

        {
            let grps = groups.lock().unwrap();
            assert_eq!(grps.len(), 2);
        }

        closer.handle.abort();
    }
}
