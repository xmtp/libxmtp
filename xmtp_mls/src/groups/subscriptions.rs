use std::pin::Pin;

use crate::api_client_wrapper::GroupFilter;
use crate::storage::group_message::StoredGroupMessage;
use crate::storage::refresh_state::EntityKind;
use futures::{Stream, StreamExt};
use xmtp_proto::api_client::XmtpMlsClient;
use xmtp_proto::xmtp::mls::api::v1::GroupMessage;

use super::{extract_message_v1, GroupError, MlsGroup};

impl<'c, ApiClient> MlsGroup<'c, ApiClient>
where
    ApiClient: XmtpMlsClient,
{
    async fn process_stream_entry(
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
            self.sync().await?;
        }

        // Load the message from the DB to handle cases where it may have been
        // already processed in another thread
        let new_message = self
            .client
            .store
            .conn()?
            .get_group_message_by_timestamp(&self.group_id, msgv1.created_ns as i64)?;

        Ok(new_message)
    }

    pub async fn stream(
        &'c self,
    ) -> Result<Pin<Box<dyn Stream<Item = StoredGroupMessage> + 'c>>, GroupError> {
        let last_cursor = self
            .client
            .store
            .conn()?
            .get_last_cursor_for_id(self.group_id.clone(), EntityKind::Group)?;

        let subscription = self
            .client
            .api_client
            .subscribe_group_messages(vec![GroupFilter::new(
                self.group_id.clone(),
                Some(last_cursor as u64),
            )])
            .await?;
        let stream = subscription
            .map(|res| async {
                match res {
                    Ok(envelope) => self.process_stream_entry(envelope).await,
                    Err(err) => Err(GroupError::Api(err)),
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
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::{builder::ClientBuilder, storage::group_message::GroupMessageKind};
    use futures::StreamExt;

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_subscribe_messages() {
        let amal = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        amal.register_identity().await.unwrap();
        let bola = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        bola.register_identity().await.unwrap();

        let amal_group = amal.create_group().unwrap();
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
    async fn test_subscribe_membership_changes() {
        let amal = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        amal.register_identity().await.unwrap();
        let bola = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        bola.register_identity().await.unwrap();

        let amal_group = amal.create_group().unwrap();

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
