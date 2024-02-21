use std::{
    collections::HashMap,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use futures::{Stream, StreamExt};
use tokio::sync::oneshot::{self, Sender};
use xmtp_proto::{api_client::XmtpMlsClient, xmtp::mls::api::v1::WelcomeMessage};

use crate::{
    api_client_wrapper::GroupFilter,
    client::{extract_welcome_message, ClientError},
    groups::{extract_group_id, GroupError, MlsGroup},
    storage::{group::StoredGroup, group_message::StoredGroupMessage},
    Client,
};

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
pub(crate) struct MessagesStreamInfo {
    group: StoredGroup,
    cursor: u64,
}

impl Clone for MessagesStreamInfo {
    fn clone(&self) -> Self {
        Self {
            group: self.group.clone(),
            cursor: self.cursor,
        }
    }
}

impl<'a, ApiClient> Client<ApiClient>
where
    ApiClient: XmtpMlsClient + 'static,
{
    fn process_streamed_welcome(
        &self,
        welcome: WelcomeMessage,
    ) -> Result<MlsGroup<ApiClient>, ClientError> {
        let welcome_v1 = extract_welcome_message(welcome)?;
        let conn = self.store.conn()?;
        let provider = self.mls_provider(&conn);

        MlsGroup::create_from_encrypted_welcome(
            self,
            &provider,
            welcome_v1.hpke_public_key.as_slice(),
            welcome_v1.data,
        )
        .map_err(|e| ClientError::Generic(e.to_string()))
    }

    pub async fn stream_conversations(
        &'a self,
    ) -> Result<Pin<Box<dyn Stream<Item = MlsGroup<ApiClient>> + Send + 'a>>, ClientError> {
        let installation_key = self.installation_public_key();
        let id_cursor = 0;

        let subscription = self
            .api_client
            .subscribe_welcome_messages(installation_key, Some(id_cursor as u64))
            .await?;

        let stream = subscription
            .map(|welcome_result| async {
                let welcome = welcome_result?;
                self.process_streamed_welcome(welcome)
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
}

impl<ApiClient> Client<ApiClient>
where
    ApiClient: XmtpMlsClient + 'static,
{
    pub fn stream_conversations_with_callback(
        client: Arc<Client<ApiClient>>,
        callback: impl Fn(MlsGroup<ApiClient>) + Send + 'static,
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
                            Some(convo) => { callback(convo) },
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

    pub(crate) async fn stream_messages_for_groups(
        client: Arc<Client<ApiClient>>,
        group_id_to_info: HashMap<Vec<u8>, MessagesStreamInfo>,
    ) -> Result<Pin<Box<dyn Stream<Item = StoredGroupMessage> + Send>>, ClientError> {
        let filters: Vec<GroupFilter> = group_id_to_info
            .iter()
            .map(|(group_id, info)| GroupFilter::new(group_id.clone(), Some(info.cursor)))
            .collect();
        let messages_subscription = client.api_client.subscribe_group_messages(filters).await?;
        let group_id_to_info_clone = group_id_to_info.clone();
        let client_clone = client.clone();
        let stream = messages_subscription
            .map(move |res| {
                let group_id_to_info_clone = group_id_to_info_clone.clone();
                let client_clone = client_clone.clone();
                async move {
                    match res {
                        Ok(envelope) => {
                            let group_id = extract_group_id(&envelope)?;
                            let stored_group = &group_id_to_info_clone
                                .get(&group_id)
                                .ok_or(ClientError::StreamInconsistency(
                                    "Received message for a non-subscribed group".to_string(),
                                ))?
                                .group;
                            MlsGroup::new(
                                client_clone.as_ref(),
                                group_id,
                                stored_group.created_at_ns,
                            )
                            .process_stream_entry(envelope)
                            .await
                        }
                        Err(err) => Err(GroupError::Api(err)),
                    }
                }
            })
            .filter_map(move |res| async {
                match res.await {
                    Ok(Some(message)) => Some(message),
                    Ok(None) => None,
                    Err(err) => {
                        log::error!("Error processing stream entry: {:?}", err);
                        None
                    }
                }
            });

        Ok(Box::pin(stream))
    }

    pub(crate) fn stream_messages_for_groups_with_callback(
        client: Arc<Client<ApiClient>>,
        group_id_to_info: HashMap<Vec<u8>, MessagesStreamInfo>,
        callback: impl Fn(StoredGroupMessage) + Send + 'static,
    ) -> Result<StreamCloser, ClientError> {
        let (close_sender, close_receiver) = oneshot::channel::<()>();
        let is_closed = Arc::new(AtomicBool::new(false));
        let is_closed_clone = is_closed.clone();

        tokio::spawn(async move {
            let mut stream = Self::stream_messages_for_groups(client, group_id_to_info)
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
        callback: impl Fn(StoredGroupMessage) + Send + 'static,
    ) -> Result<StreamCloser, ClientError> {
        client.sync_welcomes().await?; // TODO pipe cursor from welcomes sync into groups_stream
        let group_id_to_info: HashMap<Vec<u8>, MessagesStreamInfo> = client
            .store
            .conn()?
            .find_groups(None, None, None, None)?
            .into_iter()
            .map(|group| (group.id.clone(), MessagesStreamInfo { group, cursor: 0 }))
            .collect();

        Self::stream_messages_for_groups_with_callback(client, group_id_to_info, move |message| {
            callback(message)
        })

        // 0. Allow streaming messages with callback. Make stream_all_messages use callback.
        // 1. Set up groups stream to re-init message stream without cursor
        //      - Can we add a GroupFilter field to stream_all_messages, and just call it again when needed?
        // 2. Pipe cursor from groups stream to new message stream
        // 3. Pipe cursor from group sync to groups stream
        // 4. Set up new message streams instead of new groups streams

        // Set up messages stream
        // Set up groups stream.
        // Every time it changes, close the current message stream and wait for confirmation
        // Start a new messages stream using the existing cursors.
        // group_id_to_info needs to be shared between the messages stream and the groups stream
        // The groups stream needs to be able to shut down the messages stream

        // let groups_stream = Self::stream_conversations_with_callback(client.clone(), |convo| {});
        // TODO update messages_stream based on groups_stream
    }
}

#[cfg(test)]
mod tests {
    use crate::{builder::ClientBuilder, storage::group_message::StoredGroupMessage, Client};
    use futures::StreamExt;
    use std::sync::{Arc, Mutex};
    use xmtp_api_grpc::grpc_api_helper::Client as GrpcClient;
    use xmtp_cryptography::utils::generate_local_wallet;

    #[tokio::test]
    async fn test_stream_welcomes() {
        let alice = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bob = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alice_bob_group = alice.create_group(None).unwrap();

        let mut bob_stream = bob.stream_conversations().await.unwrap();
        alice_bob_group
            .add_members(vec![bob.account_address()])
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
            .add_members_by_installation_id(vec![caro.installation_public_key()])
            .await
            .unwrap();

        let bo_group = bo.create_group(None).unwrap();
        bo_group
            .add_members_by_installation_id(vec![caro.installation_public_key()])
            .await
            .unwrap();

        let messages: Arc<Mutex<Vec<StoredGroupMessage>>> = Arc::new(Mutex::new(Vec::new()));
        let messages_clone = messages.clone();
        let mut _stream = Client::<GrpcClient>::stream_all_messages_with_callback(
            Arc::new(caro),
            move |message| {
                (*messages_clone.lock().unwrap()).push(message);
            },
        )
        .await
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        alix_group.send_message("first".as_bytes()).await.unwrap();
        bo_group.send_message("second".as_bytes()).await.unwrap();
        alix_group.send_message("third".as_bytes()).await.unwrap();
        bo_group.send_message("fourth".as_bytes()).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let messages = messages.lock().unwrap();
        assert_eq!(messages[0].decrypted_message_bytes, "first".as_bytes());
        assert_eq!(messages[1].decrypted_message_bytes, "second".as_bytes());
        assert_eq!(messages[2].decrypted_message_bytes, "third".as_bytes());
        assert_eq!(messages[3].decrypted_message_bytes, "fourth".as_bytes());
    }
}
