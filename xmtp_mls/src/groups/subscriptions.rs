use futures::{Stream, StreamExt};

use prost::Message;
use std::collections::HashMap;
use tokio::sync::oneshot;

use super::MlsGroup;
use crate::{
    groups::ScopedGroupClient,
    storage::group_message::StoredGroupMessage,
    subscriptions::{stream_messages::{ProcessMessageFuture, StreamGroupMessages, MessagesStreamInfo}, SubscribeError},
    XmtpOpenMlsProvider,
};
use xmtp_proto::api_client::{trait_impls::XmtpApi, XmtpMlsStreams};
use xmtp_proto::xmtp::mls::api::v1::GroupMessage;

impl<ScopedClient: ScopedGroupClient> MlsGroup<ScopedClient> {
    /// External proxy for `process_stream_entry`
    /// Converts some `SubscribeError` variants to an Option, if they are inconsequential.
    pub async fn process_streamed_group_message(
        &self,
        provider: &XmtpOpenMlsProvider,
        envelope_bytes: Vec<u8>,
    ) -> Result<StoredGroupMessage, SubscribeError> {
        let envelope = GroupMessage::decode(envelope_bytes.as_slice())?;
        ProcessMessageFuture::new(&self.client, envelope)?
            .process()
            .await
    }

    pub async fn stream<'a>(
        &'a self,
    ) -> Result<
        impl Stream<Item = Result<StoredGroupMessage, SubscribeError>> + use<'a, ScopedClient>,
        SubscribeError,
    >
    where
        <ScopedClient as ScopedGroupClient>::ApiClient: XmtpMlsStreams + 'a,
    {
        let group_list = HashMap::from([(
            self.group_id.clone(),
            MessagesStreamInfo {
                convo_created_at_ns: self.created_at_ns,
                cursor: 0,
            },
        )]);
        Ok(StreamGroupMessages::new(&self.client, &group_list).await?)
    }

    pub fn stream_with_callback(
        client: ScopedClient,
        group_id: Vec<u8>,
        created_at_ns: i64,
        callback: impl FnMut(Result<StoredGroupMessage, SubscribeError>) + Send + 'static,
    ) -> impl crate::StreamHandle<StreamOutput = Result<(), SubscribeError>>
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
/*
/// Stream messages from groups in `group_id_to_info`
// TODO: Note when to use a None provider
#[tracing::instrument(level = "debug", skip_all)]
pub(crate) async fn stream_messages<'a, ScopedClient>(
    client: &'a ScopedClient,
    group_id_to_info: Arc<HashMap<Vec<u8>, MessagesStreamInfo>>,
) -> Result<impl Stream<Item = Result<StoredGroupMessage, SubscribeError>> + 'a, ClientError>
where
    ScopedClient: ScopedGroupClient,
    <ScopedClient as ScopedGroupClient>::ApiClient: XmtpApi + XmtpMlsStreams + 'a,
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
                let provider = client.mls_provider()?;
                let envelope = res.map_err(GroupError::from)?;
                let group_id = extract_group_id(&envelope)?;
                tracing::info!(
                    inbox_id = client.inbox_id(),
                    group_id = hex::encode(&group_id),
                    "Received message streaming payload"
                );
                let stream_info =
                    group_id_to_info
                        .get(&group_id)
                        .ok_or(ClientError::StreamInconsistency(
                            "Received message for a non-subscribed group".to_string(),
                        ))?;
                let mls_group = MlsGroup::new(client, group_id, stream_info.convo_created_at_ns);

                mls_group.process_stream_entry(&provider, envelope).await
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
*/

/// Stream messages from groups in `group_id_to_info`, passing
/// messages along to a callback.
pub(crate) fn stream_messages_with_callback<ScopedClient>(
    client: ScopedClient,
    group_id_to_info: HashMap<Vec<u8>, MessagesStreamInfo>,
    mut callback: impl FnMut(Result<StoredGroupMessage, SubscribeError>) + Send + 'static,
) -> impl crate::StreamHandle<StreamOutput = Result<(), SubscribeError>>
where
    ScopedClient: ScopedGroupClient + 'static,
    <ScopedClient as ScopedGroupClient>::ApiClient: XmtpApi + XmtpMlsStreams + 'static,
{
    let (tx, rx) = oneshot::channel();

    crate::spawn(Some(rx), async move {
        let client_ref = &client;
        let stream = StreamGroupMessages::new(client_ref, &group_id_to_info).await?;
        futures::pin_mut!(stream);
        let _ = tx.send(());
        while let Some(message) = stream.next().await {
            callback(message)
        }
        tracing::debug!("`stream_messages` stream ended, dropping stream");
        Ok::<_, SubscribeError>(())
    })
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use wasm_bindgen_test::wasm_bindgen_test;

    use super::*;
    use tokio_stream::wrappers::UnboundedReceiverStream;
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::{
        builder::ClientBuilder, groups::GroupMetadataOptions,
        storage::group_message::GroupMessageKind, utils::test::Delivery,
    };
    use futures::StreamExt;

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread", worker_threads = 10))]
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
        let provider = amal.mls_provider().unwrap();
        let message_again = amal_group
            .process_streamed_group_message(&provider, message_bytes)
            .await;

        if let Ok(message) = message_again {
            assert_eq!(message.group_id, amal_group.clone().group_id)
        } else {
            panic!("failed, message needs to equal message_again");
        }
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread", worker_threads = 10))]
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
            .sync_welcomes(&bola.mls_provider().unwrap())
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

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread", worker_threads = 10))]
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

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread", worker_threads = 10))]
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
        xmtp_common::time::sleep(core::time::Duration::from_millis(100)).await;

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
