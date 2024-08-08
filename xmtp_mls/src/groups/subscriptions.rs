use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use futures::Stream;

use super::{extract_message_v1, GroupError, MlsGroup};
use crate::storage::group_message::StoredGroupMessage;
use crate::subscriptions::{MessagesStreamInfo, StreamHandle};
use crate::XmtpApi;
use crate::{retry::Retry, retry_async, Client};
use prost::Message;
use xmtp_proto::xmtp::mls::api::v1::GroupMessage;

impl MlsGroup {
    pub(crate) async fn process_stream_entry<ApiClient>(
        &self,
        envelope: GroupMessage,
        client: Arc<Client<ApiClient>>,
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

        let client_pointer = client.clone();
        let process_result = retry_async!(
            Retry::default(),
            (async {
                let client_pointer = client_pointer.clone();
                let client_id = client_id.clone();
                let msgv1 = msgv1.clone();
                self.context
                    .store
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
                            client_pointer.as_ref(),
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
            self.sync(&client).await?;
        } else if process_result.is_err() {
            log::error!("Process stream entry {:?}", process_result.err());
        }

        // Load the message from the DB to handle cases where it may have been already processed in
        // another thread
        let new_message = self
            .context
            .store
            .conn()?
            .get_group_message_by_timestamp(&self.group_id, created_ns as i64)?;

        Ok(new_message)
    }

    pub async fn process_streamed_group_message<ApiClient>(
        &self,
        envelope_bytes: Vec<u8>,
        client: Arc<Client<ApiClient>>,
    ) -> Result<StoredGroupMessage, GroupError>
    where
        ApiClient: XmtpApi,
    {
        let envelope = GroupMessage::decode(envelope_bytes.as_slice())
            .map_err(|e| GroupError::Generic(e.to_string()))?;

        let message = self.process_stream_entry(envelope, client).await?;
        Ok(message.unwrap())
    }

    pub async fn stream<ApiClient>(
        &self,
        client: Arc<Client<ApiClient>>,
    ) -> Result<Pin<Box<dyn Stream<Item = StoredGroupMessage> + Send + '_>>, GroupError>
    where
        ApiClient: crate::XmtpApi,
    {
        Ok(client
            .stream_messages(HashMap::from([(
                self.group_id.clone(),
                MessagesStreamInfo {
                    convo_created_at_ns: self.created_at_ns,
                    cursor: 0,
                },
            )]))
            .await?)
    }

    pub fn stream_with_callback<ApiClient>(
        client: Arc<Client<ApiClient>>,
        group_id: Vec<u8>,
        created_at_ns: i64,
        callback: impl FnMut(StoredGroupMessage) + Send + 'static,
    ) -> StreamHandle<Result<(), crate::groups::ClientError>>
    where
        ApiClient: crate::XmtpApi,
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
mod tests {
    use prost::Message;
    use std::{sync::Arc, time::Duration};
    use tokio_stream::wrappers::UnboundedReceiverStream;
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::{
        builder::ClientBuilder, groups::GroupMetadataOptions,
        storage::group_message::GroupMessageKind, utils::test::Delivery,
    };
    use futures::StreamExt;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
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
            .process_streamed_group_message(message_bytes, Arc::new(amal))
            .await;

        if let Ok(message) = message_again {
            assert_eq!(message.group_id, amal_group.clone().group_id)
        } else {
            panic!("failed, message needs to equal message_again");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
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
        tokio::spawn(async move {
            let mut stream = bola_group_ptr.stream(bola_ptr).await.unwrap();
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_subscribe_multiple() {
        let amal = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        let stream = group.stream(amal.clone()).await.unwrap();

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

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_subscribe_membership_changes() {
        let amal = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        let mut stream = amal_group.stream(amal.clone()).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        amal_group
            .add_members_by_inbox_id(&amal, vec![bola.inbox_id()])
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let first_val = stream.next().await.unwrap();
        assert_eq!(first_val.kind, GroupMessageKind::MembershipChange);

        amal_group
            .send_message("hello".as_bytes(), &amal)
            .await
            .unwrap();
        let second_val = stream.next().await.unwrap();
        assert_eq!(second_val.decrypted_message_bytes, "hello".as_bytes());
    }
}
