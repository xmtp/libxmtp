use futures::{Stream, StreamExt};

use prost::Message;
use tokio::sync::oneshot;

use super::MlsGroup;
use crate::{
    groups::ScopedGroupClient,
    storage::group_message::StoredGroupMessage,
    subscriptions::{
        stream_messages::{ProcessMessageFuture, StreamGroupMessages},
        Result, SubscribeError,
    },
    types::GroupId,
};
use xmtp_proto::api_client::{trait_impls::XmtpApi, XmtpMlsStreams};
use xmtp_proto::xmtp::mls::api::v1::GroupMessage;

impl<ScopedClient: ScopedGroupClient> MlsGroup<ScopedClient> {
    /// External proxy for `process_stream_entry`
    /// Converts some `SubscribeError` variants to an Option, if they are inconsequential.
    /// Useful for streaming outside of an InboxApp, like for Push Notifications.
    /// Pulls a new provider connection.
    pub async fn process_streamed_group_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<StoredGroupMessage> {
        let envelope = GroupMessage::decode(envelope_bytes.as_slice())?;
        ProcessMessageFuture::new(&self.client, envelope)?
            .process()
            .await?
            .map(|(group, _)| group)
            .ok_or(SubscribeError::GroupMessageNotFound)
    }

    pub async fn stream<'a>(
        &'a self,
    ) -> Result<impl Stream<Item = Result<StoredGroupMessage>> + use<'a, ScopedClient>>
    where
        <ScopedClient as ScopedGroupClient>::ApiClient: XmtpMlsStreams + 'a,
    {
        StreamGroupMessages::new(&self.client, vec![self.group_id.clone().into()]).await
    }

    pub fn stream_with_callback(
        client: ScopedClient,
        group_id: Vec<u8>,
        callback: impl FnMut(Result<StoredGroupMessage>) + Send + 'static,
    ) -> impl crate::StreamHandle<StreamOutput = Result<()>>
    where
        ScopedClient: 'static,
        <ScopedClient as ScopedGroupClient>::ApiClient: XmtpMlsStreams + 'static,
    {
        stream_messages_with_callback(client, vec![group_id.into()].into_iter(), callback)
    }
}

/// Stream messages from groups in `group_id_to_info`, passing
/// messages along to a callback.
pub(crate) fn stream_messages_with_callback<ScopedClient>(
    client: ScopedClient,
    active_conversations: impl Iterator<Item = GroupId> + Send + 'static,
    mut callback: impl FnMut(Result<StoredGroupMessage>) + Send + 'static,
) -> impl crate::StreamHandle<StreamOutput = Result<()>>
where
    ScopedClient: ScopedGroupClient + 'static,
    <ScopedClient as ScopedGroupClient>::ApiClient: XmtpApi + XmtpMlsStreams + 'static,
{
    let (tx, rx) = oneshot::channel();

    crate::spawn(Some(rx), async move {
        let client_ref = &client;
        let stream = StreamGroupMessages::new(client_ref, active_conversations.collect()).await?;
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

    use std::sync::Arc;

    use super::*;
    use crate::{
        builder::ClientBuilder, groups::GroupMetadataOptions,
        storage::group_message::GroupMessageKind,
    };
    use xmtp_cryptography::utils::generate_local_wallet;

    use futures::StreamExt;
    use wasm_bindgen_test::wasm_bindgen_test;

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
        let message_again = amal_group
            .process_streamed_group_message(message_bytes)
            .await;

        if let Ok(message) = message_again {
            assert_eq!(message.group_id, amal_group.clone().group_id)
        } else {
            panic!("failed, message needs to equal message_again");
        }
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
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
        let bola_group = bola_groups.first().unwrap();

        let stream = bola_group.stream().await.unwrap();
        futures::pin_mut!(stream);

        amal_group.send_message("hello".as_bytes()).await.unwrap();
        let first_val = stream.next().await.unwrap().unwrap();
        assert_eq!(first_val.decrypted_message_bytes, "hello".as_bytes());

        amal_group.send_message("goodbye".as_bytes()).await.unwrap();
        let second_val = stream.next().await.unwrap().unwrap();
        assert_eq!(second_val.decrypted_message_bytes, "goodbye".as_bytes());
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread", worker_threads = 10))]
    async fn test_subscribe_multiple() {
        let amal = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        let stream = group.stream().await.unwrap();
        futures::pin_mut!(stream);

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

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_subscribe_membership_changes() {
        let amal = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        let stream = amal_group.stream().await.unwrap();
        futures::pin_mut!(stream);

        amal_group
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();

        let first_val = stream.next().await.unwrap().unwrap();
        assert_eq!(first_val.kind, GroupMessageKind::MembershipChange);

        amal_group.send_message("hello".as_bytes()).await.unwrap();
        let second_val = stream.next().await.unwrap().unwrap();
        assert_eq!(second_val.decrypted_message_bytes, "hello".as_bytes());
    }
}
