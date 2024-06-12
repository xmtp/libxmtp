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
use tokio::{
    sync::{
        mpsc::{self, UnboundedSender},
        oneshot::{self, Sender},
    },
    task::JoinHandle,
};
use tokio_stream::wrappers::UnboundedReceiverStream;
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
        let conn = self.store().conn()?;
        let provider = self.mls_provider(conn);

        MlsGroup::create_from_encrypted_welcome(
            self,
            &provider,
            welcome_v1.hpke_public_key.as_slice(),
            welcome_v1.data,
        )
        .await
        .map_err(|e| ClientError::Generic(e.to_string()))
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
            .map(|welcome| async {
                log::info!("Received conversation streaming payload");
                self.process_streamed_welcome(welcome?).await
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

    pub async fn stream_all_messages(
        client: Arc<Client<ApiClient>>,
    ) -> Result<impl Stream<Item = StoredGroupMessage>, ClientError> {
        let mut handle;

        //TODO:insipx backpressure
        let (tx, rx) = mpsc::unbounded_channel();

        client.sync_welcomes().await?;
        log::debug!("Synced Welcomes!!!");

        let current_groups = client.store().conn()?.find_groups(None, None, None, None)?;

        let mut group_id_to_info: HashMap<Vec<u8>, MessagesStreamInfo> = current_groups
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
        log::info!("Groups len: {:?}", group_id_to_info.len());

        handle = Self::relay_messages(client.clone(), tx.clone(), group_id_to_info.clone());

        tokio::spawn(async move {
            let client_pointer = client.clone();
            let mut convo_stream = Self::stream_conversations(&client_pointer).await?;

            loop {
                log::debug!("Selecting ....");
                // TODO:insipx We should more closely investigate whether
                // the stream mapping in `stream_conversations` is cancellation safe
                // otherwise it could lead to hard-to-find bugs
                tokio::select! {
                    Some(new_group) = convo_stream.next() => {
                        if group_id_to_info.contains_key(&new_group.group_id) {
                            continue;
                        }

                        handle.abort();
                        for info in group_id_to_info.values_mut() {
                            info.cursor = 0;
                        }
                        group_id_to_info.insert(
                            new_group.group_id,
                            MessagesStreamInfo {
                                convo_created_at_ns: new_group.created_at_ns,
                                cursor: 1,
                            },
                        );
                        handle = Self::relay_messages(client.clone(), tx.clone(), group_id_to_info.clone());
                    },
                    maybe_finished = &mut handle => {
                        match maybe_finished {
                            // if all is well it means the stream closed (our receiver is dropped
                            // or ended), our work is done
                            Ok(_) => break,
                            Err(e) => {
                                // if we have an error, it probably means we need to try and
                                // restart the stream.
                                log::error!("{}", e.to_string());
                                handle = Self::relay_messages(client.clone(), tx.clone(), group_id_to_info.clone());
                            }
                        }
                    }
                }
            }
            Ok::<_, ClientError>(())
        });

        Ok(UnboundedReceiverStream::new(rx))
    }

    pub fn stream_all_messages_with_callback(
        client: Arc<Client<ApiClient>>,
        mut callback: impl FnMut(StoredGroupMessage) + Send + Sync + 'static,
    ) -> JoinHandle<Result<(), ClientError>> {
        // make this call block until it is ready
        // otherwise we miss messages
        let (tx, rx) = tokio::sync::oneshot::channel();

        let handle = tokio::spawn(async move {
            let mut stream = Self::stream_all_messages(client).await?;
            let _ = tx.send(());
            while let Some(message) = stream.next().await {
                callback(message)
            }
            Ok(())
        });

        let _ = tokio::task::block_in_place(|| rx.blocking_recv());
        handle
    }

    fn relay_messages(
        client: Arc<Client<ApiClient>>,
        tx: UnboundedSender<StoredGroupMessage>,
        group_id_to_info: HashMap<Vec<u8>, MessagesStreamInfo>,
    ) -> JoinHandle<Result<(), ClientError>> {
        tokio::spawn(async move {
            let mut stream = client.stream_messages(group_id_to_info).await?;
            while let Some(message) = stream.next().await {
                // an error can only mean the receiver has been dropped or closed
                log::debug!(
                    "SENDING MESSAGE {}",
                    String::from_utf8_lossy(&message.decrypted_message_bytes)
                );
                if tx.send(message).is_err() {
                    log::debug!("CLOSING STREAM");
                    break;
                }
                log::debug!("Sent Message!");
            }
            Ok::<_, ClientError>(())
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{builder::ClientBuilder, storage::group_message::StoredGroupMessage, Client};
    use futures::StreamExt;
    use std::sync::{Arc, Mutex};
    use xmtp_api_grpc::grpc_api_helper::Client as GrpcClient;
    use xmtp_cryptography::utils::generate_local_wallet;

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_stream_welcomes() {
        let alice = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bob = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alice_bob_group = alice.create_group(None).unwrap();

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

        let alix_group = alix.create_group(None).unwrap();
        alix_group
            .add_members_by_inbox_id(&alix, vec![caro.inbox_id()])
            .await
            .unwrap();

        let bo_group = bo.create_group(None).unwrap();
        bo_group
            .add_members_by_inbox_id(&bo, vec![caro.inbox_id()])
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let messages: Arc<Mutex<Vec<StoredGroupMessage>>> = Arc::new(Mutex::new(Vec::new()));
        let messages_clone = messages.clone();
        Client::<GrpcClient>::stream_all_messages_with_callback(Arc::new(caro), move |message| {
            log::debug!("YOOO MESSAGES");
            (*messages_clone.lock().unwrap()).push(message);
        });

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
        for message in messages.iter() {
            println!(
                "{}",
                String::from_utf8_lossy(&message.decrypted_message_bytes)
            );
        }
        assert_eq!(messages[0].decrypted_message_bytes, b"first");
        assert_eq!(messages[1].decrypted_message_bytes, b"second");
        assert_eq!(messages[2].decrypted_message_bytes, b"third");
        assert_eq!(messages[3].decrypted_message_bytes, b"fourth");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_stream_all_messages_changing_group_list() {
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let caro = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let alix_group = alix.create_group(None).unwrap();
        alix_group
            .add_members_by_inbox_id(&alix, vec![caro.inbox_id()])
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let messages: Arc<Mutex<Vec<StoredGroupMessage>>> = Arc::new(Mutex::new(Vec::new()));
        let messages_clone = messages.clone();
        let handle =
            Client::<GrpcClient>::stream_all_messages_with_callback(caro.clone(), move |message| {
                let text = String::from_utf8(message.decrypted_message_bytes.clone())
                    .unwrap_or("<not UTF8>".to_string());
                println!("Received: {}", text);
                (*messages_clone.lock().unwrap()).push(message);
            });

        alix_group
            .send_message("first".as_bytes(), &alix)
            .await
            .unwrap();

        let bo_group = bo.create_group(None).unwrap();
        bo_group
            .add_members_by_inbox_id(&bo, vec![caro.inbox_id()])
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

        let alix_group_2 = alix.create_group(None).unwrap();
        alix_group_2
            .add_members_by_inbox_id(&alix, vec![caro.inbox_id()])
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        alix_group
            .send_message("fourth".as_bytes(), &alix)
            .await
            .unwrap();

        alix_group_2
            .send_message("fifth".as_bytes(), &alix)
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        {
            let messages = messages.lock().unwrap();
            assert_eq!(messages.len(), 5);
        }

        handle.abort();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(handle.is_finished());

        alix_group
            .send_message("first".as_bytes(), &alix)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let messages = messages.lock().unwrap();
        assert_eq!(messages.len(), 5);
    }
}
