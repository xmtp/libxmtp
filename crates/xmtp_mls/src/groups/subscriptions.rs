use super::MlsGroup;
use crate::{
    context::XmtpSharedContext,
    cursor_store::SqliteCursorStore,
    subscriptions::{
        Result, SubscribeError,
        d14n_compat::{V3OrD14n, decode_group_message},
        process_message::{ProcessFutureFactory, ProcessMessageFuture},
        stream_messages::StreamGroupMessages,
    },
};
use futures::{FutureExt, Stream, StreamExt, TryStreamExt, future, stream as future_stream};
use itertools::Itertools;
use tokio::sync::oneshot;
use xmtp_api_d14n::{
    protocol::{
        CursorStore, EnvelopeCollection, EnvelopeError, GroupMessageExtractor,
        V3GroupMessageExtractor,
    },
    stream,
};
use xmtp_common::MaybeSend;
use xmtp_common::StreamHandle;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_proto::xmtp::mls::api::v1::GroupMessage as V3GroupMessage;
use xmtp_proto::{
    ConversionError,
    types::{GroupId, GroupMessage},
};
use xmtp_proto::{api_client::XmtpMlsStreams, types::TopicCursor};

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    /// External proxy for `process_stream_entry`
    /// Useful for streaming outside of an InboxApp, like for Push Notifications.
    /// in d14n, may return multiple
    /// [`StoredGroupMessage`](xmtp_db::group_message::StoredGroupMessage)'s,
    /// since a subscription response may include many
    /// [`OriginatorEnvelope`](xmtp_proto::xmtp::xmtpv4::envelopes::OriginatorEnvelope)'s.
    /// In case d14n iceboxes the message, returns an empty vector.
    pub async fn process_streamed_group_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<Vec<StoredGroupMessage>> {
        let message = decode_group_message(envelope_bytes.as_slice())?;
        let messages: Vec<_> = match message {
            V3OrD14n::D14n(subscribe) => {
                let messages = subscribe.envelopes;
                let topics = messages.topics()?;
                let store = SqliteCursorStore::new(self.context.db());
                let cursor: TopicCursor = store
                    .latest_for_topics(&mut topics.iter())
                    .map_err(SubscribeError::dyn_err)?
                    .into();
                stream::try_extractor::<_, GroupMessageExtractor>(stream::ordered(
                    future_stream::once(future::ready(Ok::<_, EnvelopeError>(messages))),
                    store,
                    cursor,
                ))
                .try_collect()
                .now_or_never()
                .unwrap_or(Ok(Vec::new()))
            }
            V3OrD14n::V3(message) => {
                let s: Vec<GroupMessage> =
                    stream::try_extractor::<_, V3GroupMessageExtractor>(future_stream::iter(vec![
                        Ok::<_, EnvelopeError>(vec![message]),
                    ]))
                    .try_collect::<Vec<Option<GroupMessage>>>()
                    .now_or_never()
                    .expect("stream must not fail because it is statically created with one item")?
                    .into_iter()
                    .map(|m| {
                        m.ok_or_else(|| {
                            // this is a bug if it occurs. group message extractor
                            // must be able to extract a message from a statically created
                            // group message.
                            let err = ConversionError::Missing {
                                item: "group_message",
                                r#type: std::any::type_name::<V3GroupMessage>(),
                            };
                            SubscribeError::dyn_err(err)
                        })
                    })
                    .try_collect()?;
                Ok(s)
            }
        }?;

        future_stream::iter(messages.into_iter())
            .then(|msg| async move {
                ProcessMessageFuture::new(self.context.clone())
                    .create(msg)
                    .await?
                    .message
                    .ok_or(SubscribeError::GroupMessageNotFound)
            })
            .try_collect()
            .await
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
        Context::Db: 'static,
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
        Context: 'static,
        Context::ApiClient: XmtpMlsStreams + 'static,
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
    Context: XmtpSharedContext + 'static,
    Context::ApiClient: XmtpMlsStreams + 'static,
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
    use crate::groups::send_message_opts::SendMessageOpts;
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use std::sync::Arc;

    use crate::builder::ClientBuilder;
    use crate::{Client, db::mock::MockDbQuery, worker::WorkerRunner};
    use prost::Message as ProstMessage;
    use tls_codec::Serialize as TlsSerialize;
    use xmtp_common::Generate;
    use xmtp_db::group_message::GroupMessageKind;
    use xmtp_proto::xmtp::mls::api::v1::{GroupMessage as V3GroupMessage, group_message};
    use xmtp_proto::xmtp::mls::api::v1::{GroupMessageInput, group_message_input};
    use xmtp_proto::xmtp::xmtpv4::envelopes::{
        AuthenticatedData, ClientEnvelope, OriginatorEnvelope, PayerEnvelope,
        UnsignedOriginatorEnvelope, client_envelope, originator_envelope,
    };
    use xmtp_proto::xmtp::xmtpv4::message_api::SubscribeEnvelopesResponse;

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
        amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

        // Get bola's version of the same group
        let bola_groups = bola.sync_welcomes().await.unwrap();
        let bola_group = bola_groups.first().unwrap();

        let stream = bola_group.stream().await.unwrap();
        futures::pin_mut!(stream);

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

    #[rstest::rstest]
    #[xmtp_common::test]
    #[timeout(Duration::from_secs(5))]
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

    #[rstest::rstest]
    #[xmtp_common::test(flavor = "multi_thread", worker_threads = 1)]
    #[timeout(Duration::from_secs(5))]
    async fn test_process_streamed_group_message_v3(
        #[from(crate::test::mock::context)] mut context: crate::test::mock::NewMockContext,
    ) {
        context.store.expect_db().returning(|| {
            let mut mock_db = MockDbQuery::new();
            mock_db.expect_find_group().returning(|_| Ok(None));
            mock_db.expect_insert_or_replace_group().returning(Ok);
            mock_db
                .expect_insert_or_replace_consent_records()
                .returning(|_| Ok(vec![]));
            mock_db.expect_group_cursors().returning(|| Ok(vec![]));
            mock_db
                .expect_get_last_cursor_for_ids()
                .returning(|_, _| Ok(std::collections::HashMap::new()));
            mock_db
                .expect_get_group_message_by_timestamp()
                .returning(|group_id, timestamp| {
                    use xmtp_db::group_message::{
                        ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage,
                    };
                    Ok(Some(StoredGroupMessage {
                        id: xmtp_common::rand_vec::<32>(),
                        group_id: group_id.as_ref().to_vec(),
                        decrypted_message_bytes: b"test message".to_vec(),
                        sent_at_ns: timestamp,
                        kind: GroupMessageKind::Application,
                        sender_installation_id: xmtp_common::rand_vec::<32>(),
                        sender_inbox_id: "test inbox".into(),
                        delivery_status: DeliveryStatus::Published,
                        content_type: ContentType::Text,
                        version_major: 0,
                        version_minor: 0,
                        authority_id: "testauthority".to_string(),
                        reference_id: None,
                        sequence_id: 1,
                        originator_id: 0,
                        expire_at_ns: None,
                        inserted_at_ns: 0,
                        should_push: true,
                    }))
                });
            mock_db
        });

        let local_events = context.local_events.clone();
        let workers = Arc::new(WorkerRunner::default());
        let installation_id = context.installation_id().clone();
        let client = Client {
            context: Arc::new(context),
            installation_id,
            local_events,
            workers,
        };

        let group = client.create_group(None, None).unwrap();

        let fake_message = xmtp_common::FakeMlsApplicationMessage::generate();
        let mls_message_out = openmls::prelude::MlsMessageOut::from(fake_message);
        let message_data = mls_message_out.tls_serialize_detached().unwrap();

        let v3_message = V3GroupMessage {
            version: Some(group_message::Version::V1(group_message::V1 {
                id: 1,
                created_ns: 1000000,
                group_id: group.group_id.clone(),
                data: message_data,
                sender_hmac: vec![],
                should_push: false,
                is_commit: false,
            })),
        };

        let mut envelope_bytes = Vec::new();
        v3_message.encode(&mut envelope_bytes).unwrap();

        let result = group.process_streamed_group_message(envelope_bytes).await;

        assert!(result.is_ok(), "V3 processing should succeed: {:?}", result);
        let messages = result.unwrap();
        assert!(!messages.is_empty(), "Should have messages");
        assert_eq!(messages.len(), 1, "Should have exactly one message");
    }

    #[rstest::rstest]
    #[xmtp_common::test(flavor = "multi_thread", worker_threads = 1)]
    #[timeout(Duration::from_secs(5))]
    async fn test_process_streamed_group_message_d14n(
        #[from(crate::test::mock::context)] mut context: crate::test::mock::NewMockContext,
    ) {
        context.store.expect_db().returning(|| {
            let mut mock_db = MockDbQuery::new();
            mock_db.expect_find_group().returning(|_| Ok(None));
            mock_db.expect_insert_or_replace_group().returning(Ok);
            mock_db
                .expect_insert_or_replace_consent_records()
                .returning(|_| Ok(vec![]));
            mock_db.expect_group_cursors().returning(|| Ok(vec![]));
            mock_db
                .expect_get_last_cursor_for_ids()
                .returning(|_, _| Ok(std::collections::HashMap::new()));
            mock_db
                .expect_get_group_message_by_timestamp()
                .returning(|group_id, timestamp| {
                    use xmtp_db::group_message::{
                        ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage,
                    };
                    Ok(Some(StoredGroupMessage {
                        id: xmtp_common::rand_vec::<32>(),
                        group_id: group_id.as_ref().to_vec(),
                        decrypted_message_bytes: b"test message".to_vec(),
                        sent_at_ns: timestamp,
                        kind: GroupMessageKind::Application,
                        sender_installation_id: xmtp_common::rand_vec::<32>(),
                        sender_inbox_id: "test inbox".into(),
                        delivery_status: DeliveryStatus::Published,
                        content_type: ContentType::Text,
                        version_major: 0,
                        version_minor: 0,
                        authority_id: "testauthority".to_string(),
                        reference_id: None,
                        sequence_id: 1,
                        originator_id: 1,
                        expire_at_ns: None,
                        inserted_at_ns: 0,
                        should_push: true,
                    }))
                });
            mock_db.expect_future_dependents().returning(|_| Ok(vec![]));
            mock_db.expect_update_cursor().returning(|_, _, _| Ok(true));
            mock_db
        });

        let local_events = context.local_events.clone();
        let workers = Arc::new(WorkerRunner::default());
        let installation_id = context.installation_id().clone();
        let client = Client {
            context: Arc::new(context),
            installation_id,
            local_events,
            workers,
        };

        let group = client.create_group(None, None).unwrap();

        let fake_message = xmtp_common::FakeMlsApplicationMessage::generate();
        let mls_message_out = openmls::prelude::MlsMessageOut::from(fake_message);
        let message_data = mls_message_out.tls_serialize_detached().unwrap();

        let group_message_input = GroupMessageInput {
            version: Some(group_message_input::Version::V1(group_message_input::V1 {
                data: message_data,
                sender_hmac: vec![],
                should_push: false,
            })),
        };

        let client_envelope = ClientEnvelope {
            aad: Some(AuthenticatedData {
                target_topic: group.group_id.clone(),
                depends_on: None,
            }),
            payload: Some(client_envelope::Payload::GroupMessage(group_message_input)),
        };

        let mut client_envelope_bytes = Vec::new();
        client_envelope.encode(&mut client_envelope_bytes).unwrap();

        let payer_envelope = PayerEnvelope {
            unsigned_client_envelope: client_envelope_bytes,
            payer_signature: None,
            target_originator: 1,
            message_retention_days: 30,
        };

        let mut payer_envelope_bytes = Vec::new();
        payer_envelope.encode(&mut payer_envelope_bytes).unwrap();

        let unsigned_originator_envelope = UnsignedOriginatorEnvelope {
            originator_node_id: 1,
            originator_sequence_id: 1,
            originator_ns: 1000000,
            payer_envelope_bytes,
            base_fee_picodollars: 0,
            congestion_fee_picodollars: 0,
            expiry_unixtime: 0,
        };

        let mut unsigned_originator_envelope_bytes = Vec::new();
        unsigned_originator_envelope
            .encode(&mut unsigned_originator_envelope_bytes)
            .unwrap();

        let originator_envelope = OriginatorEnvelope {
            unsigned_originator_envelope: unsigned_originator_envelope_bytes,
            proof: Some(originator_envelope::Proof::OriginatorSignature(
                Default::default(),
            )),
        };

        let d14n_response = SubscribeEnvelopesResponse {
            envelopes: vec![originator_envelope],
        };

        let mut envelope_bytes = Vec::new();
        d14n_response.encode(&mut envelope_bytes).unwrap();

        let result = group.process_streamed_group_message(envelope_bytes).await;

        assert!(
            result.is_ok(),
            "D14n processing should succeed: {:?}",
            result
        );
        let messages = result.unwrap();
        assert!(!messages.is_empty(), "Should have messages");
        assert_eq!(messages.len(), 1, "Should have exactly one message");
    }
}
