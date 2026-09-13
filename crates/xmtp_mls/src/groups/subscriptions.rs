use super::MlsGroup;
use crate::{
    context::XmtpSharedContext,
    subscriptions::{
        Result, stream_messages::StreamGroupMessages, watchdog::spawn_watchdog_stream,
    },
};
use futures::Stream;
use prost::Message;
use xmtp_proto::backend_v1::ServerEnvelope;

use xmtp_common::MaybeSend;
use xmtp_common::StreamHandle;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_proto::api_client::XmtpMlsStreams;
use xmtp_proto::types::GroupId;

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext + 'static,
{
    /// Use a push envelope only as a target for ordered receipt and processing.
    pub async fn process_streamed_group_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<Vec<StoredGroupMessage>> {
        use xmtp_db::prelude::*;
        let wire = ServerEnvelope::decode(envelope_bytes.as_slice())?;
        let meta = wire
            .meta
            .as_ref()
            .ok_or(xmtp_api::ApiError::InvalidResponse("group metadata"))?;
        let (topic, cursor, _) = xmtp_api_backend::envelope::metadata(
            meta,
            xmtp_proto::types::TopicKind::GroupMessagesV1,
        )?;
        if topic != xmtp_proto::types::Topic::new_group_message(self.group_id) {
            return Err(xmtp_api::ApiError::InvalidResponse("group topic").into());
        }
        crate::subscriptions::barrier::wait_through(&self.context, [(topic, cursor)].into(), None)
            .await
            .map_err(super::GroupError::from)?;
        Ok(self
            .context
            .db()
            .get_group_message_by_cursor(self.group_id, cursor)?
            .into_iter()
            .collect())
    }

    #[tracing::instrument(err, skip_all, fields(operation = "stream.stream_group_messages"))]
    pub async fn stream<'a>(
        &'a self,
    ) -> Result<impl Stream<Item = Result<StoredGroupMessage>> + use<'a, Context>>
    where
        Context::ApiClient: XmtpMlsStreams + 'a,
    {
        StreamGroupMessages::new(&self.context, vec![self.group_id]).await
    }

    /// create a stream that is not attached to any lifetime
    #[tracing::instrument(
        err,
        skip_all,
        fields(operation = "stream.stream_group_messages_owned")
    )]
    pub async fn stream_owned(
        &self,
    ) -> Result<impl Stream<Item = Result<StoredGroupMessage>> + 'static>
    where
        Context: 'static,
        Context::ApiClient: XmtpMlsStreams + 'static,
        Context::Db: 'static,
    {
        StreamGroupMessages::new_owned(self.context.clone(), vec![self.group_id]).await
    }

    pub fn stream_with_callback(
        context: Context,
        group_id: GroupId,
        callback: impl FnMut(Result<StoredGroupMessage>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<()>>
    where
        Context: 'static,
        Context::ApiClient: XmtpMlsStreams + 'static,
    {
        stream_messages_with_callback(
            context.clone(),
            vec![group_id].into_iter(),
            callback,
            on_close,
        )
    }
}

/// Deliver stored messages for these groups and share ordered network receipt.
pub(crate) fn stream_messages_with_callback<Context>(
    context: Context,
    active_conversations: impl Iterator<Item = GroupId> + MaybeSend + 'static,
    callback: impl FnMut(Result<StoredGroupMessage>) + MaybeSend + 'static,
    on_close: impl FnOnce() + MaybeSend + 'static,
) -> impl StreamHandle<StreamOutput = Result<()>>
where
    Context: XmtpSharedContext + 'static,
    Context::ApiClient: XmtpMlsStreams + 'static,
    Context::Db: 'static,
{
    let cancel = context.cancellation_token().clone();
    let groups: Vec<GroupId> = active_conversations.collect();
    // Reopening reads saved D. A dropped, unacknowledged item remains available.
    spawn_watchdog_stream(
        cancel,
        "stream_messages",
        move || {
            let context = context.clone();
            let groups = groups.clone();
            async move { StreamGroupMessages::new_owned(context, groups).await }
        },
        callback,
        on_close,
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::context::XmtpSharedContext;
    use crate::groups::send_message_opts::SendMessageOpts;
    use futures::StreamExt;
    use std::sync::Arc;

    use crate::builder::ClientBuilder;
    use prost::Message as ProstMessage;
    use std::time::Duration;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_db::group_message::GroupMessageKind;

    #[xmtp_common::timeout(Duration::from_secs(10))]
    #[rstest::rstest]
    #[xmtp_common::test(flavor = "current_thread")]
    async fn test_subscribe_messages() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let amal_group = amal.create_group(None, None).unwrap();
        // Add bola
        amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

        // Get bola's version of the same group
        let bola_groups = bola.sync_welcomes().await.unwrap();
        let bola_group = bola_groups.first().unwrap();
        bola_group.receive().await.unwrap();
        let retained = bola_group.find_messages(&Default::default()).unwrap();

        let stream = bola_group.stream().await.unwrap();
        futures::pin_mut!(stream);
        for expected in retained {
            assert_eq!(stream.next().await.unwrap().unwrap(), expected);
        }

        amal_group
            .send_message("hello".as_bytes(), SendMessageOpts::default())
            .await
            .unwrap();
        let first_val = stream.next().await.unwrap().unwrap();
        assert_eq!(first_val.decrypted_message_bytes, "hello".as_bytes());

        amal_group
            .send_message("goodbye".as_bytes(), SendMessageOpts::default())
            .await
            .unwrap();
        let second_val = stream.next().await.unwrap().unwrap();
        assert_eq!(second_val.decrypted_message_bytes, "goodbye".as_bytes());
    }

    // TODO: THIS TESTS ALSO LOSES MESSAGES
    #[xmtp_common::timeout(Duration::from_secs(10))]
    #[rstest::rstest]
    #[xmtp_common::test(flavor = "multi_thread")]
    #[cfg_attr(target_arch = "wasm32", ignore)]
    async fn test_subscribe_multiple() {
        let amal = Arc::new(ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await);
        let group = amal.create_group(None, None).unwrap();

        let stream = group.stream().await.unwrap();
        futures::pin_mut!(stream);

        for i in 0..10 {
            group
                .send_message(
                    format!("hello {}", i).as_bytes(),
                    SendMessageOpts::default(),
                )
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

    #[xmtp_common::timeout(Duration::from_secs(5))]
    #[rstest::rstest]
    #[xmtp_common::test]
    async fn test_subscribe_membership_changes() {
        let amal = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let amal_group = amal.create_group(None, None).unwrap();

        let stream = amal_group.stream().await.unwrap();
        futures::pin_mut!(stream);

        amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

        let first_val = stream.next().await.unwrap().unwrap();
        assert_eq!(first_val.kind, GroupMessageKind::MembershipChange);

        amal_group
            .send_message("hello".as_bytes(), SendMessageOpts::default())
            .await
            .unwrap();
        let second_val = stream.next().await.unwrap().unwrap();
        assert_eq!(second_val.decrypted_message_bytes, "hello".as_bytes());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_process_streamed_group_message() {
        crate::tester!(alix);
        crate::tester!(bo);
        let group = alix.create_group(None, None)?;
        group.add_members(&[bo.inbox_id()]).await?;
        let bo_groups = bo.sync_welcomes().await?;
        let bo_group = bo_groups.first().unwrap();
        group
            .send_message(b"test message", SendMessageOpts::default())
            .await?;
        let envelopes = alix
            .context
            .api()
            .query_all(
                std::collections::HashMap::from([(
                    xmtp_proto::types::Topic::new_group_message(group.group_id),
                    xmtp_proto::types::Cursor(0),
                )]),
                xmtp_configuration::BACKEND_DEFAULT_MAX_QUERY_LIMIT as u32,
            )
            .await?;
        let envelope = envelopes.last().unwrap();
        let result = bo_group
            .process_streamed_group_message(envelope.encode_to_vec())
            .await;
        assert!(
            result.is_ok(),
            "Backend processing must succeed: {result:?}"
        );
        let messages = result?;
        assert!(!messages.is_empty());
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].decrypted_message_bytes, b"test message");
    }
}
