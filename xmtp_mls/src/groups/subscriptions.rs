use std::sync::Arc;

use super::MlsGroup;
use crate::{
    context::XmtpMlsLocalContext,
    subscriptions::{
        process_message::{ProcessFutureFactory, ProcessMessageFuture},
        stream_messages::{MessageStreamError, StreamGroupMessages},
        Result, SubscribeError,
    },
};
use xmtp_common::types::GroupId;
use xmtp_db::{group_message::StoredGroupMessage, XmtpDb};

use futures::{Stream, StreamExt};
use prost::Message;
use tokio::sync::oneshot;
use xmtp_common::StreamHandle;
use xmtp_proto::api_client::{trait_impls::XmtpApi, XmtpMlsStreams};
use xmtp_proto::xmtp::mls::api::v1::GroupMessage;

impl<ApiClient, Db> MlsGroup<ApiClient, Db>
where
    ApiClient: XmtpApi,
    Db: XmtpDb,
{
    /// External proxy for `process_stream_entry`
    /// Converts some `SubscribeError` variants to an Option, if they are inconsequential.
    /// Useful for streaming outside of an InboxApp, like for Push Notifications.
    /// Pulls a new provider connection.
    pub async fn process_streamed_group_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<StoredGroupMessage> {
        use crate::subscriptions::stream_messages::extract_message_v1;
        let envelope = GroupMessage::decode(envelope_bytes.as_slice())?;
        let msg = extract_message_v1(envelope).ok_or(MessageStreamError::InvalidPayload)?;
        ProcessMessageFuture::new(self.context.clone())
            .create(msg)
            .await?
            .message
            .ok_or(SubscribeError::GroupMessageNotFound)
    }

    pub async fn stream<'a>(
        &'a self,
    ) -> Result<impl Stream<Item = Result<StoredGroupMessage>> + use<'a, ApiClient, Db>>
    where
        ApiClient: XmtpMlsStreams + 'a,
    {
        StreamGroupMessages::new(&self.context, vec![self.group_id.clone().into()]).await
    }

    /// create a stream that is not attached to any lifetime
    pub async fn stream_owned(
        &self,
    ) -> Result<impl Stream<Item = Result<StoredGroupMessage>> + 'static>
    where
        ApiClient: XmtpMlsStreams + Send + Sync + 'static,
        Db: Send + Sync + 'static,
    {
        StreamGroupMessages::new_owned(self.context.clone(), vec![self.group_id.clone().into()])
            .await
    }

    pub fn stream_with_callback(
        context: Arc<XmtpMlsLocalContext<ApiClient, Db>>,
        group_id: Vec<u8>,
        #[cfg(target_arch = "wasm32")] callback: impl FnMut(Result<StoredGroupMessage>) + 'static,
        #[cfg(not(target_arch = "wasm32"))] callback: impl FnMut(Result<StoredGroupMessage>)
            + Send
            + 'static,
        on_close: impl FnOnce() + Send + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>>
    where
        ApiClient: 'static,
        ApiClient: XmtpMlsStreams + 'static,
        Db: 'static,
    {
        stream_messages_with_callback(
            context,
            vec![group_id.into()].into_iter(),
            callback,
            on_close,
        )
    }
}

// TODO: there's a better way than #[cfg]
/// Stream messages from groups in `group_id_to_info`, passing
/// messages along to a callback.
pub(crate) fn stream_messages_with_callback<ApiClient, Db>(
    context: Arc<XmtpMlsLocalContext<ApiClient, Db>>,
    #[cfg(not(target_arch = "wasm32"))] active_conversations: impl Iterator<Item = GroupId>
        + Send
        + 'static,
    #[cfg(target_arch = "wasm32")] active_conversations: impl Iterator<Item = GroupId> + 'static,
    #[cfg(target_arch = "wasm32")] mut callback: impl FnMut(Result<StoredGroupMessage>) + 'static,
    #[cfg(not(target_arch = "wasm32"))] mut callback: impl FnMut(Result<StoredGroupMessage>)
        + Send
        + 'static,
    on_close: impl FnOnce() + Send + 'static,
) -> impl StreamHandle<StreamOutput = Result<()>>
where
    ApiClient: XmtpApi + XmtpMlsStreams + 'static,
    Db: XmtpDb + 'static,
{
    let (tx, rx) = oneshot::channel();

    xmtp_common::spawn(Some(rx), async move {
        let context_ref = &context;
        let stream = StreamGroupMessages::new(context_ref, active_conversations.collect()).await?;
        futures::pin_mut!(stream);
        let _ = tx.send(());
        while let Some(message) = stream.next().await {
            callback(message)
        }
        tracing::debug!("`stream_messages` stream ended, dropping stream");
        on_close();
        Ok::<_, SubscribeError>(())
    })
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use std::sync::Arc;

    use super::*;
    use crate::builder::ClientBuilder;
    use xmtp_db::group_message::GroupMessageKind;

    use std::time::Duration;
    use xmtp_cryptography::utils::generate_local_wallet;

    use futures::StreamExt;

    #[rstest::rstest]
    #[xmtp_common::test]
    #[timeout(Duration::from_secs(10))]
    async fn test_decode_group_message_bytes() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let amal_group = amal.create_group(None, None).unwrap();
        // Add bola
        amal_group
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();

        amal_group.send_message("hello".as_bytes()).await.unwrap();
        let messages = amal
            .context
            .api_client
            .query_group_messages(amal_group.clone().group_id, None, None)
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

    #[rstest::rstest]
    #[xmtp_common::test(flavor = "current_thread")]
    #[timeout(Duration::from_secs(10))]
    async fn test_subscribe_messages() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let amal_group = amal.create_group(None, None).unwrap();
        // Add bola
        amal_group
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();

        // Get bola's version of the same group
        let bola_groups = bola.sync_welcomes().await.unwrap();
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

    // TODO: THIS TESTS ALSO LOSES MESSAGES
    #[rstest::rstest]
    #[xmtp_common::test(flavor = "multi_thread")]
    #[timeout(Duration::from_secs(10))]
    #[cfg_attr(target_arch = "wasm32", ignore)]
    async fn test_subscribe_multiple() {
        let amal = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let group = amal.create_group(None, None).unwrap();

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

    #[rstest::rstest]
    #[xmtp_common::test]
    #[timeout(Duration::from_secs(5))]
    async fn test_subscribe_membership_changes() {
        let amal = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let amal_group = amal.create_group(None, None).unwrap();

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
