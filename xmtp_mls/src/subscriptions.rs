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
// use xmtp_api_grpc::grpc_api_helper::Client as ApiClient;
use xmtp_proto::{api_client::XmtpMlsClient, xmtp::mls::api::v1::WelcomeMessage};

use crate::{
    api_client_wrapper::GroupFilter,
    client::{extract_welcome_message, ClientError},
    groups::{extract_group_id, GroupError, MlsGroup},
    storage::group_message::StoredGroupMessage,
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
struct MessagesStreamInfo<'c, ApiClient> {
    group: MlsGroup<'c, ApiClient>,
    cursor: u64,
}

impl<'c, ApiClient> Clone for MessagesStreamInfo<'c, ApiClient> {
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

    pub fn stream_conversations_with_callback(
        client: Arc<Client<ApiClient>>,
        callback: impl Fn(MlsGroup<ApiClient>) -> () + Send + 'static,
    ) -> Result<StreamCloser, ClientError> {
        let (close_sender, close_receiver) = oneshot::channel::<()>();
        let is_closed = Arc::new(AtomicBool::new(false));
        let is_closed_clone = is_closed.clone();
        // let client = client.clone();

        tokio::spawn(async move {
            // let client = client.as_ref();
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

    pub async fn stream_all_messages(
        &'a self,
    ) -> Result<Pin<Box<dyn Stream<Item = StoredGroupMessage> + 'a + Send>>, ClientError> {
        let groups_stream = self.stream_conversations().await?;
        self.sync_welcomes().await?;
        let group_id_to_info: HashMap<Vec<u8>, MessagesStreamInfo<'a, ApiClient>> = self
            .find_groups(None, None, None, None)?
            .into_iter()
            .map(|group| {
                (
                    group.group_id.clone(),
                    MessagesStreamInfo { group, cursor: 0 },
                )
            })
            .collect();

        // loop {
        //     match groups_stream.next() {
        //         Some(convo) => callback.on_conversation(Arc::new(FfiGroup {
        //             inner_client: inner_client.clone(),
        //             group_id: convo.group_id,
        //             created_at_ns: convo.created_at_ns,
        //         })),
        //         None => break,
        //     }
        // }

        let filters: Vec<GroupFilter> = group_id_to_info
            .iter()
            .map(|(group_id, info)| GroupFilter::new(group_id.clone(), Some(info.cursor)))
            .collect();
        let messages_subscription = self.api_client.subscribe_group_messages(filters).await?;
        let group_id_to_info_clone = group_id_to_info.clone();
        let stream = messages_subscription
            .map(move |res| {
                let group_id_to_info_clone = group_id_to_info_clone.clone();
                async move {
                    match res {
                        Ok(envelope) => {
                            let group_id = extract_group_id(&envelope)?;
                            group_id_to_info_clone
                                .get(&group_id)
                                .ok_or(ClientError::StreamInconsistency(
                                    "Received message for a non-subscribed group".to_string(),
                                ))?
                                .group
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
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::builder::ClientBuilder;

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

        let mut stream = caro.stream_all_messages().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        alix_group.send_message("first".as_bytes()).await.unwrap();
        bo_group.send_message("second".as_bytes()).await.unwrap();
        alix_group.send_message("third".as_bytes()).await.unwrap();
        bo_group.send_message("fourth".as_bytes()).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        assert_eq!(
            stream.next().await.unwrap().decrypted_message_bytes,
            "first".as_bytes()
        );
        assert_eq!(
            stream.next().await.unwrap().decrypted_message_bytes,
            "second".as_bytes()
        );
        assert_eq!(
            stream.next().await.unwrap().decrypted_message_bytes,
            "third".as_bytes()
        );
        assert_eq!(
            stream.next().await.unwrap().decrypted_message_bytes,
            "fourth".as_bytes()
        );
    }
}
