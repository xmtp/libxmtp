use std::collections::HashMap;
use std::sync::Arc;

use futures::Stream;

use super::{extract_message_v1, GroupError, MlsGroup};
use crate::storage::group_message::StoredGroupMessage;
use crate::storage::refresh_state::EntityKind;
use crate::storage::StorageError;
use crate::subscriptions::MessagesStreamInfo;
use crate::XmtpApi;
use crate::{retry::Retry, retry_async, Client};
use prost::Message;
use xmtp_proto::xmtp::mls::api::v1::GroupMessage;

impl MlsGroup {
    pub(crate) async fn process_stream_entry<ApiClient>(
        &self,
        envelope: GroupMessage,
        client: &Client<ApiClient>,
    ) -> Result<Option<StoredGroupMessage>, GroupError>
    where
        ApiClient: XmtpApi,
    {
        let msgv1 = extract_message_v1(envelope)?;
        let msg_id = msgv1.id;
        let client_id = client.inbox_id();
        log::info!(
            "client [{}]  is about to process streamed envelope: [{}]",
            &client_id.clone(),
            &msg_id
        );
        let created_ns = msgv1.created_ns;

        if !self.has_already_synced(msg_id).await? {
            let process_result = retry_async!(
                Retry::default(),
                (async {
                    let client_id = client_id.clone();
                    let msgv1 = msgv1.clone();
                    self.context
                        .store()
                        .transaction_async(|provider| async move {
                            let mut openmls_group = self.load_mls_group(&provider)?;

                            // Attempt processing immediately, but fail if the message is not an Application Message
                            // Returning an error should roll back the DB tx
                            log::info!(
                                "current epoch for [{}] in process_stream_entry() is Epoch: [{}]",
                                client_id,
                                openmls_group.epoch()
                            );

                            self.process_message(
                                client,
                                &mut openmls_group,
                                &provider,
                                &msgv1,
                                false,
                            )
                            .await
                            .map_err(GroupError::ReceiveError)
                        })
                        .await
                })
            );

            if let Some(GroupError::ReceiveError(_)) = process_result.as_ref().err() {
                // Swallow errors here, since another process may have successfully saved the message
                // to the DB
                match self.sync_with_conn(&client.mls_provider()?, client).await {
                    Ok(_) => {
                        log::debug!("Sync triggered by streamed message successful")
                    }
                    Err(err) => {
                        log::warn!("Sync triggered by streamed message failed: {}", err);
                    }
                };
            } else if process_result.is_err() {
                log::error!("Process stream entry {:?}", process_result.err());
            }
        }

        // Load the message from the DB to handle cases where it may have been already processed in
        // another thread
        let new_message = self
            .context
            .store()
            .conn()?
            .get_group_message_by_timestamp(&self.group_id, created_ns as i64)?;

        Ok(new_message)
    }

    // Checks if a message has already been processed through a sync
    async fn has_already_synced(&self, id: u64) -> Result<bool, GroupError> {
        let check_for_last_cursor = || -> Result<i64, StorageError> {
            let conn = self.context.store().conn()?;
            conn.get_last_cursor_for_id(&self.group_id, EntityKind::Group)
        };

        let last_id = retry_async!(Retry::default(), (async { check_for_last_cursor() }))?;
        Ok(last_id >= id as i64)
    }

    pub async fn process_streamed_group_message<ApiClient>(
        &self,
        envelope_bytes: Vec<u8>,
        client: &Client<ApiClient>,
    ) -> Result<StoredGroupMessage, GroupError>
    where
        ApiClient: XmtpApi,
    {
        let envelope = GroupMessage::decode(envelope_bytes.as_slice())
            .map_err(|e| GroupError::Generic(e.to_string()))?;

        let message = self.process_stream_entry(envelope, client).await?;
        message.ok_or(GroupError::MissingMessage)
    }

    pub async fn stream<'a, ApiClient>(
        &'a self,
        client: &'a Client<ApiClient>,
    ) -> Result<impl Stream<Item = StoredGroupMessage> + '_, GroupError>
    where
        ApiClient: crate::XmtpApi + 'static,
    {
        Ok(client
            .stream_messages(Arc::new(HashMap::from([(
                self.group_id.clone(),
                MessagesStreamInfo {
                    convo_created_at_ns: self.created_at_ns,
                    cursor: 0,
                },
            )])))
            .await?)
    }

    pub fn stream_with_callback<ApiClient>(
        client: Arc<Client<ApiClient>>,
        group_id: Vec<u8>,
        created_at_ns: i64,
        callback: impl FnMut(StoredGroupMessage) + Send + 'static,
    ) -> impl crate::StreamHandle<StreamOutput = Result<(), crate::groups::ClientError>>
    where
        ApiClient: crate::XmtpApi + 'static,
    {
        Client::<ApiClient>::stream_messages_with_callback(
            client,
            HashMap::from([(
                group_id,
                MessagesStreamInfo {
                    convo_created_at_ns: created_at_ns,
                    cursor: 0,
                },
            )]),
            callback,
        )
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use core::time::Duration;
    use tokio_stream::wrappers::UnboundedReceiverStream;
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::{
        builder::ClientBuilder, groups::GroupMetadataOptions,
        storage::group_message::GroupMessageKind, utils::test::Delivery,
    };
    use futures::StreamExt;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 1)
    )]
    async fn test_decode_group_message_bytes() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        // Add bola
        amal_group
            .add_members_by_inbox_id(&amal, vec![bola.inbox_id()])
            .await
            .unwrap();

        amal_group
            .send_message("hello".as_bytes(), &amal)
            .await
            .unwrap();
        let messages = amal
            .api_client
            .query_group_messages(amal_group.clone().group_id, None)
            .await
            .expect("read topic");
        let message = messages.first().unwrap();
        let mut message_bytes: Vec<u8> = Vec::new();
        message.encode(&mut message_bytes).unwrap();
        let message_again = amal_group
            .process_streamed_group_message(message_bytes, &amal)
            .await;

        if let Ok(message) = message_again {
            assert_eq!(message.group_id, amal_group.clone().group_id)
        } else {
            panic!("failed, message needs to equal message_again");
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 10)
    )]
    async fn test_subscribe_messages() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        // Add bola
        amal_group
            .add_members_by_inbox_id(&amal, vec![bola.inbox_id()])
            .await
            .unwrap();

        // Get bola's version of the same group
        let bola_groups = bola.sync_welcomes().await.unwrap();
        let bola_group = Arc::new(bola_groups.first().unwrap().clone());

        let bola_ptr = bola.clone();
        let bola_group_ptr = bola_group.clone();
        let notify = Delivery::new(Some(Duration::from_secs(10)));
        let notify_ptr = notify.clone();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut stream = UnboundedReceiverStream::new(rx);
        let _ = crate::spawn(None, async move {
            let stream = bola_group_ptr.stream(&bola_ptr).await.unwrap();
            futures::pin_mut!(stream);
            while let Some(item) = stream.next().await {
                let _ = tx.send(item);
                notify_ptr.notify_one();
            }
        });

        amal_group
            .send_message("hello".as_bytes(), &amal)
            .await
            .unwrap();
        notify
            .wait_for_delivery()
            .await
            .expect("timed out waiting for first message");
        let first_val = stream.next().await.unwrap();
        assert_eq!(first_val.decrypted_message_bytes, "hello".as_bytes());

        amal_group
            .send_message("goodbye".as_bytes(), &amal)
            .await
            .unwrap();

        notify
            .wait_for_delivery()
            .await
            .expect("timed out waiting for second message");
        let second_val = stream.next().await.unwrap();
        assert_eq!(second_val.decrypted_message_bytes, "goodbye".as_bytes());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 10)
    )]
    async fn test_subscribe_multiple() {
        let amal = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let group = Arc::new(
            amal.create_group(None, GroupMetadataOptions::default())
                .unwrap(),
        );

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
        let amal_ptr = amal.clone();
        let group_ptr = group.clone();
        let _ = crate::spawn(None, async move {
            let stream = group_ptr.stream(&amal_ptr).await.unwrap();
            futures::pin_mut!(stream);
            while let Some(item) = stream.next().await {
                let _ = tx.send(item);
            }
        });

        for i in 0..10 {
            group
                .send_message(format!("hello {}", i).as_bytes(), &amal)
                .await
                .unwrap();
        }

        // Limit the stream so that it closes after 10 messages
        let limited_stream = stream.take(10);
        let values = limited_stream.collect::<Vec<_>>().await;
        assert_eq!(values.len(), 10);
        for value in values {
            assert!(value
                .decrypted_message_bytes
                .starts_with("hello".as_bytes()));
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 5)
    )]
    async fn test_subscribe_membership_changes() {
        let amal = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        let amal_ptr = amal.clone();
        let amal_group_ptr = amal_group.clone();
        let notify = Delivery::new(Some(Duration::from_secs(20)));
        let notify_ptr = notify.clone();

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let (start_tx, start_rx) = tokio::sync::oneshot::channel();
        let mut stream = UnboundedReceiverStream::new(rx);
        let _ = crate::spawn(None, async move {
            let stream = amal_group_ptr.stream(&amal_ptr).await.unwrap();
            let _ = start_tx.send(());
            futures::pin_mut!(stream);
            while let Some(item) = stream.next().await {
                let _ = tx.send(item);
                notify_ptr.notify_one();
            }
        });
        // just to make sure stream is started
        let _ = start_rx.await;
        // Adding in a sleep, since the HTTP API client may acknowledge requests before they are ready
        crate::sleep(core::time::Duration::from_millis(100)).await;

        amal_group
            .add_members_by_inbox_id(&amal, vec![bola.inbox_id()])
            .await
            .unwrap();
        notify
            .wait_for_delivery()
            .await
            .expect("Never received group membership change from stream");
        let first_val = stream.next().await.unwrap();
        assert_eq!(first_val.kind, GroupMessageKind::MembershipChange);

        amal_group
            .send_message("hello".as_bytes(), &amal)
            .await
            .unwrap();
        notify
            .wait_for_delivery()
            .await
            .expect("Never received second message from stream");
        let second_val = stream.next().await.unwrap();
        assert_eq!(second_val.decrypted_message_bytes, "hello".as_bytes());
    }
}
