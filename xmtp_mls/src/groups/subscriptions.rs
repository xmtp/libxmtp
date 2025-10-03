use super::MlsGroup;
use crate::{
    context::XmtpSharedContext,
    subscriptions::{
        Result, SubscribeError,
        process_message::{ProcessFutureFactory, ProcessMessageFuture},
        stream_messages::StreamGroupMessages,
    },
};
use futures::{Stream, StreamExt};
use prost::Message;
use tokio::sync::oneshot;
use xmtp_api_d14n::protocol::{Extractor, ProtocolEnvelope as _};
use xmtp_common::MaybeSend;
use xmtp_common::StreamHandle;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_proto::api_client::XmtpMlsStreams;
use xmtp_proto::{types::GroupId, xmtp::mls::api::v1::GroupMessage};

impl<Context> MlsGroup<Context>
where
    Context: Send + Sync + XmtpSharedContext,
{
    /// External proxy for `process_stream_entry`
    /// Converts some `SubscribeError` variants to an Option, if they are inconsequential.
    /// Useful for streaming outside of an InboxApp, like for Push Notifications.
    /// Pulls a new provider connection.
    pub async fn process_streamed_group_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<StoredGroupMessage> {
        let envelope = GroupMessage::decode(envelope_bytes.as_slice())?;
        // TODO:d14n pair the v3 with the d14n extractor to be able to extract
        // both message versions. this can be done with a tuple, i.e
        // let mut extractor = (V3, D14n);
        // or d14n crate should just create a type alias for such an extractor
        let mut extractor = xmtp_api_d14n::protocol::V3GroupMessageExtractor::default();
        envelope.accept(&mut extractor)?;
        let message: xmtp_proto::types::GroupMessage = extractor.get()?.unwrap();
        ProcessMessageFuture::new(self.context.clone())
            .create(message)
            .await?
            .message
            .ok_or(SubscribeError::GroupMessageNotFound)
    }

    pub async fn stream<'a>(
        &'a self,
    ) -> Result<impl Stream<Item = Result<StoredGroupMessage>> + use<'a, Context>>
    where
        Context::ApiClient: XmtpMlsStreams + 'a,
    {
        StreamGroupMessages::new(&self.context, vec![self.group_id.clone().into()]).await
    }

    /// create a stream that is not attached to any lifetime
    pub async fn stream_owned(
        &self,
    ) -> Result<impl Stream<Item = Result<StoredGroupMessage>> + 'static>
    where
        Context: 'static,
        Context::ApiClient: XmtpMlsStreams + 'static,
        Context::Db: Send + Sync + 'static,
    {
        StreamGroupMessages::new_owned(self.context.clone(), vec![self.group_id.clone().into()])
            .await
    }

    pub fn stream_with_callback(
        context: Context,
        group_id: Vec<u8>,
        callback: impl FnMut(Result<StoredGroupMessage>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>>
    where
        Context: Send + Sync + 'static,
        Context::ApiClient: XmtpMlsStreams + 'static,
        Context::MlsStorage: Send + Sync,
    {
        stream_messages_with_callback(
            context.clone(),
            vec![group_id.into()].into_iter(),
            callback,
            on_close,
        )
    }
}

// TODO: there's a better way than #[cfg]
/// Stream messages from groups in `group_id_to_info`, passing
/// messages along to a callback.
pub(crate) fn stream_messages_with_callback<Context>(
    context: Context,
    active_conversations: impl Iterator<Item = GroupId> + MaybeSend + 'static,
    mut callback: impl FnMut(Result<StoredGroupMessage>) + MaybeSend + 'static,
    on_close: impl FnOnce() + MaybeSend + 'static,
) -> impl StreamHandle<StreamOutput = Result<()>>
where
    Context: Sync + Send + XmtpSharedContext + 'static,
    Context::ApiClient: XmtpMlsStreams + 'static,
    Context::MlsStorage: Send + Sync,
{
    let (tx, rx) = oneshot::channel();

    xmtp_common::spawn(Some(rx), async move {
        let stream = match StreamGroupMessages::new(&context, active_conversations.collect()).await
        {
            Ok(stream) => stream,
            Err(e) => {
                tracing::warn!("Failed to create group message stream, closing: {}", e);
                on_close();
                return Ok::<_, SubscribeError>(());
            }
        };
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

    use crate::builder::ClientBuilder;
    use xmtp_db::group_message::GroupMessageKind;

    use std::time::Duration;
    use xmtp_cryptography::utils::generate_local_wallet;

    use futures::StreamExt;

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
        let amal = Arc::new(ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await);
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
            assert!(
                value
                    .unwrap()
                    .decrypted_message_bytes
                    .starts_with("hello".as_bytes())
            );
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
