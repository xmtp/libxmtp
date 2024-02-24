use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use futures::Stream;

use xmtp_proto::{api_client::XmtpMlsClient, xmtp::mls::api::v1::GroupMessage};

use super::{extract_message_v1, GroupError, MlsGroup};
use crate::storage::group_message::StoredGroupMessage;
use crate::subscriptions::{MessagesStreamInfo, StreamCloser};
use crate::Client;

impl<'c, ApiClient> MlsGroup<'c, ApiClient>
where
    ApiClient: XmtpMlsClient,
{
    pub(crate) async fn process_stream_entry(
        &self,
        envelope: GroupMessage,
    ) -> Result<Option<StoredGroupMessage>, GroupError> {
        let msgv1 = extract_message_v1(envelope)?;

        let process_result = self.client.store.transaction(|provider| {
            let mut openmls_group = self.load_mls_group(&provider)?;
            // Attempt processing immediately, but fail if the message is not an Application Message
            // Returning an error should roll back the DB tx
            self.process_message(&mut openmls_group, &provider, &msgv1, false)
                .map_err(GroupError::ReceiveError)
        });

        if let Some(GroupError::ReceiveError(_)) = process_result.err() {
            log::info!("Re-syncing due to unreadable messaging stream payload");
            self.sync().await?;
        }

        // Load the message from the DB to handle cases where it may have been already processed in
        // another thread
        let new_message = self
            .client
            .store
            .conn()?
            .get_group_message_by_timestamp(&self.group_id, msgv1.created_ns as i64)?;

        Ok(new_message)
    }

    pub async fn stream(
        &'c self,
    ) -> Result<Pin<Box<dyn Stream<Item = StoredGroupMessage> + 'c + Send>>, GroupError> {
        Ok(self
            .client
            .stream_messages(HashMap::from([(
                self.group_id.clone(),
                MessagesStreamInfo {
                    convo_created_at_ns: self.created_at_ns,
                    cursor: 0,
                },
            )]))
            .await?)
    }

    pub async fn stream_with_callback(
        client: Arc<Client<ApiClient>>,
        group_id: Vec<u8>,
        created_at_ns: i64,
        mut callback: impl FnMut(StoredGroupMessage) + Send + 'static,
    ) -> Result<StreamCloser, GroupError> {
        Ok(Client::<ApiClient>::stream_messages_with_callback(
            client,
            HashMap::from([(
                group_id,
                MessagesStreamInfo {
                    convo_created_at_ns: created_at_ns,
                    cursor: 0,
                },
            )]),
            move |message| callback(message),
        )?)
    }
}

#[cfg(test)]
mod tests {
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::{builder::ClientBuilder, storage::group_message::GroupMessageKind};
    use futures::StreamExt;

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_subscribe_messages() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let amal_group = amal.create_group(None).unwrap();
        // Add bola
        amal_group
            .add_members_by_installation_id(vec![bola.installation_public_key()])
            .await
            .unwrap();

        // Get bola's version of the same group
        let bola_groups = bola.sync_welcomes().await.unwrap();
        let bola_group = bola_groups.first().unwrap();

        let mut stream = bola_group.stream().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        amal_group.send_message("hello".as_bytes()).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let first_val = stream.next().await.unwrap();
        assert_eq!(first_val.decrypted_message_bytes, "hello".as_bytes());

        amal_group.send_message("goodbye".as_bytes()).await.unwrap();

        let second_val = stream.next().await.unwrap();
        assert_eq!(second_val.decrypted_message_bytes, "goodbye".as_bytes());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_subscribe_multiple() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let group = amal.create_group(None).unwrap();

        let stream = group.stream().await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        for i in 0..10 {
            group
                .send_message(format!("hello {}", i).as_bytes())
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
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let amal_group = amal.create_group(None).unwrap();

        let mut stream = amal_group.stream().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        amal_group
            .add_members_by_installation_id(vec![bola.installation_public_key()])
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let first_val = stream.next().await.unwrap();
        assert_eq!(first_val.kind, GroupMessageKind::MembershipChange);

        amal_group.send_message("hello".as_bytes()).await.unwrap();
        let second_val = stream.next().await.unwrap();
        assert_eq!(second_val.decrypted_message_bytes, "hello".as_bytes());
    }
}
