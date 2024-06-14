use std::{
    collections::HashMap,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use futures::{Stream, StreamExt};
use prost::Message;
use tokio::sync::oneshot::{self, Sender};
use xmtp_proto::xmtp::mls::api::v1::WelcomeMessage;

use crate::{
    api::GroupFilter,
    client::{extract_welcome_message, ClientError},
    groups::{extract_group_id, GroupError, MlsGroup},
    storage::group_message::StoredGroupMessage,
    Client, XmtpApi,
};

// TODO simplify FfiStreamCloser + StreamCloser duplication
pub struct StreamCloser {
    pub close_fn: Arc<Mutex<Option<Sender<()>>>>,
    pub is_closed_atomic: Arc<AtomicBool>,
}

impl StreamCloser {
    pub fn end(&self) {
        match self.close_fn.lock() {
            Ok(mut close_fn_option) => {
                let _ = close_fn_option.take().map(|close_fn| close_fn.send(()));
            }
            _ => {
                log::warn!("close_fn already closed");
            }
        }
    }

    pub fn is_closed(&self) -> bool {
        self.is_closed_atomic.load(Ordering::Relaxed)
    }
}

#[derive(Clone)]
pub(crate) struct MessagesStreamInfo {
    pub convo_created_at_ns: i64,
    pub cursor: u64,
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
        // let conn = self.store().conn()?;
        // let provider = self.mls_provider(conn);

        let creation_result = self
            .context
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
            .await;

        if let Some(err) = creation_result.as_ref().err() {
            return Err(ClientError::Generic(err.to_string()));
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
        let installation_key = self.installation_public_key();
        let id_cursor = 0;

        let subscription = self
            .api_client
            .subscribe_welcome_messages(installation_key, Some(id_cursor as u64))
            .await?;

        let stream = subscription
            .map(|welcome_result| async {
                log::info!("Received conversation streaming payload");
                let welcome = welcome_result?;
                self.process_streamed_welcome(welcome).await
            })
            .filter_map(|res| async {
                match res.await {
                    Ok(group) => Some(group),
                    Err(err) => {
                        log::error!("Error processing stream entry: {:?}", err);
                        None
                    }
                }
            });

        Ok(Box::pin(stream))
    }

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
                            // TODO update cursor
                            MlsGroup::new(context, group_id, stream_info.convo_created_at_ns)
                                .process_stream_entry(envelope, client)
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
        mut on_close_callback: impl FnMut() + Send + 'static,
    ) -> Result<StreamCloser, ClientError> {
        let (close_sender, close_receiver) = oneshot::channel::<()>();
        let is_closed = Arc::new(AtomicBool::new(false));
        let is_closed_clone = is_closed.clone();

        tokio::spawn(async move {
            let mut stream = client.stream_conversations().await.unwrap();
            let mut close_receiver = close_receiver;
            loop {
                tokio::select! {
                    item = stream.next() => {
                        match item {
                            Some(convo) => { convo_callback(convo) },
                            None => break
                        }
                    }
                    _ = &mut close_receiver => {
                        on_close_callback();
                        break;
                    }
                }
            }
            is_closed_clone.store(true, Ordering::Relaxed);
            log::info!("closing stream");
        });

        Ok(StreamCloser {
            close_fn: Arc::new(Mutex::new(Some(close_sender))),
            is_closed_atomic: is_closed,
        })
    }

    pub(crate) fn stream_messages_with_callback(
        client: Arc<Client<ApiClient>>,
        group_id_to_info: HashMap<Vec<u8>, MessagesStreamInfo>,
        mut callback: impl FnMut(StoredGroupMessage) + Send + 'static,
    ) -> Result<StreamCloser, ClientError> {
        let (close_sender, close_receiver) = oneshot::channel::<()>();
        let is_closed = Arc::new(AtomicBool::new(false));

        let is_closed_clone = is_closed.clone();
        tokio::spawn(async move {
            let mut stream = Self::stream_messages(client, group_id_to_info)
                .await
                .unwrap();
            let mut close_receiver = close_receiver;
            loop {
                tokio::select! {
                    item = stream.next() => {
                        match item {
                            Some(message) => callback(message),
                            None => break
                        }
                    }
                    _ = &mut close_receiver => {
                        break;
                    }
                }
            }
            is_closed_clone.store(true, Ordering::Relaxed);
            log::info!("closing stream");
        });

        Ok(StreamCloser {
            close_fn: Arc::new(Mutex::new(Some(close_sender))),
            is_closed_atomic: is_closed,
        })
    }

    pub async fn stream_all_messages_with_callback(
        client: Arc<Client<ApiClient>>,
        callback: impl FnMut(StoredGroupMessage) + Send + Sync + 'static,
    ) -> Result<StreamCloser, ClientError> {
        client.sync_welcomes().await?; // TODO pipe cursor from welcomes sync into groups_stream
        Self::stream_all_messages_with_callback_sync(client, callback)
    }

    /// Requires a sync welcomes before use
    pub fn stream_all_messages_with_callback_sync(
        client: Arc<Client<ApiClient>>,
        callback: impl FnMut(StoredGroupMessage) + Send + Sync + 'static,
    ) -> Result<StreamCloser, ClientError> {
        let callback = Arc::new(Mutex::new(callback));

        let mut group_id_to_info: HashMap<Vec<u8>, MessagesStreamInfo> = client
            .store()
            .conn()?
            .find_groups(None, None, None, None)?
            .into_iter()
            .map(|group| {
                (
                    group.id.clone(),
                    MessagesStreamInfo {
                        convo_created_at_ns: group.created_at_ns,
                        cursor: 0,
                    },
                )
            })
            .collect();

        let callback_clone = callback.clone();
        let messages_stream_closer_mutex =
            Arc::new(Mutex::new(Self::stream_messages_with_callback(
                client.clone(),
                group_id_to_info.clone(),
                move |message| callback_clone.lock().unwrap()(message), // TODO fix unwrap
            )?));
        let messages_stream_closer_mutex_clone = messages_stream_closer_mutex.clone();
        let groups_stream_closer = Self::stream_conversations_with_callback(
            client.clone(),
            move |convo| {
                // TODO make sure key comparison works correctly
                if group_id_to_info.contains_key(&convo.group_id) {
                    return;
                }
                // Close existing message stream
                // TODO remove unwrap
                let mut messages_stream_closer = messages_stream_closer_mutex.lock().unwrap();
                messages_stream_closer.end();

                // Set up new stream. For existing groups, stream new messages only by unsetting the cursor
                for info in group_id_to_info.values_mut() {
                    info.cursor = 0;
                }
                group_id_to_info.insert(
                    convo.group_id,
                    MessagesStreamInfo {
                        convo_created_at_ns: convo.created_at_ns,
                        cursor: 1, // For the new group, stream all messages since the group was created
                    },
                );

                // Open new message stream
                let callback_clone = callback.clone();
                *messages_stream_closer = Self::stream_messages_with_callback(
                    client.clone(),
                    group_id_to_info.clone(),
                    move |message| callback_clone.lock().unwrap()(message), // TODO fix unwrap
                )
                .unwrap(); // TODO fix unwrap
            },
            move || {
                messages_stream_closer_mutex_clone.lock().unwrap().end();
            },
        )?;

        Ok(groups_stream_closer)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        builder::ClientBuilder, groups::GroupMetadataOptions,
        storage::group_message::StoredGroupMessage, Client,
    };
    use futures::StreamExt;
    use std::sync::{Arc, Mutex};
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
        let stream = Client::<GrpcClient>::stream_all_messages_with_callback(
            Arc::new(caro),
            move |message| {
                (*messages_clone.lock().unwrap()).push(message);
            },
        )
        .await
        .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        alix_group
            .send_message("first".as_bytes(), &alix)
            .await
            .unwrap();
        bo_group
            .send_message("second".as_bytes(), &bo)
            .await
            .unwrap();
        alix_group
            .send_message("third".as_bytes(), &alix)
            .await
            .unwrap();
        bo_group
            .send_message("fourth".as_bytes(), &bo)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let messages = messages.lock().unwrap();
        assert_eq!(messages[0].decrypted_message_bytes, "first".as_bytes());
        assert_eq!(messages[1].decrypted_message_bytes, "second".as_bytes());
        assert_eq!(messages[2].decrypted_message_bytes, "third".as_bytes());
        assert_eq!(messages[3].decrypted_message_bytes, "fourth".as_bytes());

        stream.end();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_stream_all_messages_changing_group_list() {
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let caro = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_group
            .add_members_by_inbox_id(&alix, vec![caro.inbox_id()])
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let messages: Arc<Mutex<Vec<StoredGroupMessage>>> = Arc::new(Mutex::new(Vec::new()));
        let messages_clone = messages.clone();
        let stream =
            Client::<GrpcClient>::stream_all_messages_with_callback(caro.clone(), move |message| {
                let text = String::from_utf8(message.decrypted_message_bytes.clone())
                    .unwrap_or("<not UTF8>".to_string());
                println!("Received: {}", text);
                (*messages_clone.lock().unwrap()).push(message);
            })
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        alix_group
            .send_message("first".as_bytes(), &alix)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let bo_group = bo
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        bo_group
            .add_members_by_inbox_id(&bo, vec![caro.inbox_id()])
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        bo_group
            .send_message("second".as_bytes(), &bo)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        alix_group
            .send_message("third".as_bytes(), &alix)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let alix_group_2 = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_group_2
            .add_members_by_inbox_id(&alix, vec![caro.inbox_id()])
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        alix_group
            .send_message("fourth".as_bytes(), &alix)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        alix_group_2
            .send_message("fifth".as_bytes(), &alix)
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        {
            let messages = messages.lock().unwrap();
            assert_eq!(messages.len(), 5);
        }

        stream.end();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(stream.is_closed());

        alix_group
            .send_message("first".as_bytes(), &alix)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let messages = messages.lock().unwrap();
        assert_eq!(messages.len(), 5);
    }
}
