use futures::{Stream, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::oneshot;
use xmtp_proto::api_client::trait_impls::XmtpApi;
use xmtp_proto::api_client::XmtpMlsStreams;

use super::{extract_message_v1, GroupError, MlsGroup, ScopedGroupClient};
use crate::api::GroupFilter;
use crate::client::ClientError;
use crate::groups::extract_group_id;
use crate::storage::group_message::StoredGroupMessage;
use crate::storage::refresh_state::EntityKind;
use crate::storage::StorageError;
use crate::subscriptions::MessagesStreamInfo;
use crate::subscriptions::SubscribeError;
use crate::{retry::Retry, retry_async};
use prost::Message;
use xmtp_proto::xmtp::mls::api::v1::GroupMessage;

impl<ScopedClient: ScopedGroupClient> MlsGroup<ScopedClient> {
    /// Internal stream processing function
    pub(crate) async fn process_stream_entry(
        &self,
        envelope: GroupMessage,
    ) -> Result<StoredGroupMessage, SubscribeError> {
        let msgv1 = extract_message_v1(envelope)?;
        let msg_id = msgv1.id;
        let client_id = self.client.inbox_id();
        tracing::info!(
            inbox_id = self.client.inbox_id(),
            group_id = hex::encode(&self.group_id),
            msg_id = msgv1.id,
            "client [{}]  is about to process streamed envelope: [{}]",
            &client_id,
            &msg_id
        );
        let created_ns = msgv1.created_ns;

        if !self.has_already_synced(msg_id).await? {
            let process_result = retry_async!(
                Retry::default(),
                (async {
                    let client_id = &client_id;
                    let msgv1 = &msgv1;
                    self.context()
                        .store()
                        .transaction_async(|provider| async move {
                            let mut openmls_group = self.load_mls_group(&provider)?;

                            // Attempt processing immediately, but fail if the message is not an Application Message
                            // Returning an error should roll back the DB tx
                            tracing::info!(
                                inbox_id = self.client.inbox_id(),
                                group_id = hex::encode(&self.group_id),
                                current_epoch = openmls_group.epoch().as_u64(),
                                msg_id = msgv1.id,
                                "current epoch for [{}] in process_stream_entry() is Epoch: [{}]",
                                client_id,
                                openmls_group.epoch()
                            );

                            self.process_message(&mut openmls_group, &provider, msgv1, false)
                                .await
                                // NOTE: We want to make sure we retry an error in process_message
                                .map_err(SubscribeError::Receive)
                        })
                        .await
                })
            );

            if let Err(SubscribeError::Receive(_)) = process_result {
                tracing::debug!(
                    inbox_id = self.client.inbox_id(),
                    group_id = hex::encode(&self.group_id),
                    msg_id = msgv1.id,
                    "attempting recovery sync"
                );
                // Swallow errors here, since another process may have successfully saved the message
                // to the DB
                if let Err(err) = self.sync_with_conn(&self.client.mls_provider()?).await {
                    tracing::warn!(
                        inbox_id = self.client.inbox_id(),
                        group_id = hex::encode(&self.group_id),
                        msg_id = msgv1.id,
                        err = %err,
                        "recovery sync triggered by streamed message failed: {}", err
                    );
                } else {
                    tracing::debug!(
                        inbox_id = self.client.inbox_id(),
                        group_id = hex::encode(&self.group_id),
                        msg_id = msgv1.id,
                        "recovery sync triggered by streamed message successful"
                    )
                }
            } else if let Err(e) = process_result {
                tracing::error!(
                    inbox_id = self.client.inbox_id(),
                    group_id = hex::encode(&self.group_id),
                    msg_id = msgv1.id,
                    err = %e,
                    "process stream entry {:?}", e
                );
            }
        }

        // Load the message from the DB to handle cases where it may have been already processed in
        // another thread
        let new_message = self
            .context()
            .store()
            .conn()?
            .get_group_message_by_timestamp(&self.group_id, created_ns as i64)?
            .ok_or(SubscribeError::GroupMessageNotFound)?;

        Ok(new_message)
    }

    // Checks if a message has already been processed through a sync
    async fn has_already_synced(&self, id: u64) -> Result<bool, GroupError> {
        let check_for_last_cursor = || -> Result<i64, StorageError> {
            let conn = self.context().store().conn()?;
            conn.get_last_cursor_for_id(&self.group_id, EntityKind::Group)
        };

        let last_id = retry_async!(Retry::default(), (async { check_for_last_cursor() }))?;
        Ok(last_id >= id as i64)
    }

    /// External proxy for `process_stream_entry`
    /// Converts some `SubscribeError` variants to an Option, if they are inconsequential.
    pub async fn process_streamed_group_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<StoredGroupMessage, SubscribeError> {
        let envelope = GroupMessage::decode(envelope_bytes.as_slice())?;
        self.process_stream_entry(envelope).await
    }

    pub async fn stream(
        &self,
    ) -> Result<
        impl Stream<Item = Result<StoredGroupMessage, SubscribeError>> + use<'_, ScopedClient>,
        ClientError,
    >
    where
        <ScopedClient as ScopedGroupClient>::ApiClient: XmtpMlsStreams + 'static,
    {
        let group_list = HashMap::from([(
            self.group_id.clone(),
            MessagesStreamInfo {
                convo_created_at_ns: self.created_at_ns,
                cursor: 0,
            },
        )]);
        stream_messages(&*self.client, Arc::new(group_list)).await
    }

    pub fn stream_with_callback(
        client: ScopedClient,
        group_id: Vec<u8>,
        created_at_ns: i64,
        callback: impl FnMut(Result<StoredGroupMessage, SubscribeError>) + Send + 'static,
    ) -> impl crate::StreamHandle<StreamOutput = Result<(), crate::groups::ClientError>>
    where
        ScopedClient: 'static,
        <ScopedClient as ScopedGroupClient>::ApiClient: XmtpMlsStreams + 'static,
    {
        let group_list = HashMap::from([(
            group_id,
            MessagesStreamInfo {
                convo_created_at_ns: created_at_ns,
                cursor: 0,
            },
        )]);
        stream_messages_with_callback(client, group_list, callback)
    }
}

/// Stream messages from groups in `group_id_to_info`
pub(crate) async fn stream_messages<ScopedClient>(
    client: &ScopedClient,
    group_id_to_info: Arc<HashMap<Vec<u8>, MessagesStreamInfo>>,
) -> Result<impl Stream<Item = Result<StoredGroupMessage, SubscribeError>> + '_, ClientError>
where
    ScopedClient: ScopedGroupClient,
    <ScopedClient as ScopedGroupClient>::ApiClient: XmtpApi + XmtpMlsStreams + 'static,
{
    let filters: Vec<GroupFilter> = group_id_to_info
        .iter()
        .map(|(group_id, info)| GroupFilter::new(group_id.clone(), Some(info.cursor)))
        .collect();

    let messages_subscription = client.api().subscribe_group_messages(filters).await?;

    let stream = messages_subscription
        .then(move |res| {
            let group_id_to_info = group_id_to_info.clone();
            async move {
                let envelope = res.map_err(GroupError::from)?;
                tracing::info!("Received message streaming payload");
                let group_id = extract_group_id(&envelope)?;
                tracing::info!("Extracted group id {}", hex::encode(&group_id));
                let stream_info =
                    group_id_to_info
                        .get(&group_id)
                        .ok_or(ClientError::StreamInconsistency(
                            "Received message for a non-subscribed group".to_string(),
                        ))?;
                let mls_group = MlsGroup::new(client, group_id, stream_info.convo_created_at_ns);
                mls_group.process_stream_entry(envelope).await
            }
        })
        .inspect(|e| {
            if matches!(e, Err(SubscribeError::GroupMessageNotFound)) {
                tracing::warn!("Skipped message streaming payload");
            }
        })
        .filter(|e| {
            futures::future::ready(!matches!(e, Err(SubscribeError::GroupMessageNotFound)))
        });

    Ok(stream)
}

/// Stream messages from groups in `group_id_to_info`, passing
/// messages along to a callback.
pub(crate) fn stream_messages_with_callback<ScopedClient>(
    client: ScopedClient,
    group_id_to_info: HashMap<Vec<u8>, MessagesStreamInfo>,
    mut callback: impl FnMut(Result<StoredGroupMessage, SubscribeError>) + Send + 'static,
) -> impl crate::StreamHandle<StreamOutput = Result<(), ClientError>>
where
    ScopedClient: ScopedGroupClient + 'static,
    <ScopedClient as ScopedGroupClient>::ApiClient: XmtpApi + XmtpMlsStreams + 'static,
{
    let (tx, rx) = oneshot::channel();

    crate::spawn(Some(rx), async move {
        let stream = stream_messages(&client, Arc::new(group_id_to_info)).await?;
        futures::pin_mut!(stream);
        let _ = tx.send(());
        while let Some(message) = stream.next().await {
            callback(message)
        }
        tracing::debug!("`stream_messages` stream ended, dropping stream");
        Ok::<_, ClientError>(())
    })
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
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
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();

        amal_group.send_message("hello".as_bytes()).await.unwrap();
        let messages = amal
            .api_client
            .query_group_messages(amal_group.clone().group_id, None)
            .await
            .expect("read topic");
        let message = messages.first().unwrap();
        let mut message_bytes: Vec<u8> = Vec::new();
        message.encode(&mut message_bytes).unwrap();
        let message_again = amal_group
            .process_streamed_group_message(message_bytes)
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
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();

        // Get bola's version of the same group
        let bola_groups = bola
            .sync_welcomes(&bola.store().conn().unwrap())
            .await
            .unwrap();
        let bola_group = Arc::new(bola_groups.first().unwrap().clone());

        let bola_group_ptr = bola_group.clone();
        let notify = Delivery::new(Some(10));
        let notify_ptr = notify.clone();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut stream = UnboundedReceiverStream::new(rx);
        crate::spawn(None, async move {
            let stream = bola_group_ptr.stream().await.unwrap();
            futures::pin_mut!(stream);
            while let Some(item) = stream.next().await {
                let _ = tx.send(item);
                notify_ptr.notify_one();
            }
        });

        amal_group.send_message("hello".as_bytes()).await.unwrap();
        notify
            .wait_for_delivery()
            .await
            .expect("timed out waiting for first message");
        let first_val = stream.next().await.unwrap().unwrap();
        assert_eq!(first_val.decrypted_message_bytes, "hello".as_bytes());

        amal_group.send_message("goodbye".as_bytes()).await.unwrap();

        notify
            .wait_for_delivery()
            .await
            .expect("timed out waiting for second message");
        let second_val = stream.next().await.unwrap().unwrap();
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
        let group_ptr = group.clone();
        crate::spawn(None, async move {
            let stream = group_ptr.stream().await.unwrap();
            futures::pin_mut!(stream);
            while let Some(item) = stream.next().await {
                let _ = tx.send(item);
            }
        });

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
                .unwrap()
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

        let amal_group_ptr = amal_group.clone();
        let notify = Delivery::new(Some(20));
        let notify_ptr = notify.clone();

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let (start_tx, start_rx) = tokio::sync::oneshot::channel();
        let mut stream = UnboundedReceiverStream::new(rx);
        crate::spawn(None, async move {
            let stream = amal_group_ptr.stream().await.unwrap();
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
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();
        notify
            .wait_for_delivery()
            .await
            .expect("Never received group membership change from stream");
        let first_val = stream.next().await.unwrap().unwrap();
        assert_eq!(first_val.kind, GroupMessageKind::MembershipChange);

        amal_group.send_message("hello".as_bytes()).await.unwrap();
        notify
            .wait_for_delivery()
            .await
            .expect("Never received second message from stream");
        let second_val = stream.next().await.unwrap().unwrap();
        assert_eq!(second_val.decrypted_message_bytes, "hello".as_bytes());
    }
}
