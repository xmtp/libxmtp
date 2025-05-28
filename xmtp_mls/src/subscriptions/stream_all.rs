use std::{
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
};

use crate::{
    context::{XmtpContextProvider, XmtpMlsLocalContext},
    subscriptions::stream_messages::MessagesApiSubscription,
    t,
};
use crate::{groups::welcome_sync::WelcomeService, subscriptions::WorkerEvent};

use xmtp_db::{
    events::{Details, Event},
    group::{ConversationType, GroupQueryArgs},
    group_message::StoredGroupMessage,
    XmtpDb,
};
use xmtp_proto::api_client::{trait_impls::XmtpApi, XmtpMlsStreams};

use super::{
    stream_conversations::{StreamConversations, WelcomesApiSubscription},
    stream_messages::StreamGroupMessages,
    Result, SubscribeError,
};
use crate::groups::MlsGroup;
use futures::stream::Stream;
use xmtp_common::types::GroupId;
use xmtp_db::{consent_record::ConsentState, group::StoredGroup};

use pin_project_lite::pin_project;

pin_project! {
    pub(super) struct StreamAllMessages<'a, ApiClient, Db, Conversations, Messages> {
        #[pin] conversations: Conversations,
        #[pin] messages: Messages,
        context: &'a XmtpMlsLocalContext<ApiClient, Db>,
        conversation_type: Option<ConversationType>,
        sync_groups: Vec<Vec<u8>>
    }
}

impl<'a, A, D>
    StreamAllMessages<
        'a,
        A,
        D,
        StreamConversations<'a, A, D, WelcomesApiSubscription<'a, A>>,
        StreamGroupMessages<'a, A, D, MessagesApiSubscription<'a, A>>,
    >
where
    A: XmtpApi + XmtpMlsStreams + Send + Sync + 'a,
    D: XmtpDb + Send + Sync + 'a,
{
    pub async fn new(
        context: &'a Arc<XmtpMlsLocalContext<A, D>>,
        conversation_type: Option<ConversationType>,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<Self> {
        let (active_conversations, sync_groups) = async {
            let provider = context.mls_provider();
            WelcomeService::new(context.clone()).sync_welcomes().await?;

            t!(
                Event::MsgStreamConnect,
                Details::MsgStreamConnect {
                    conversation_type,
                    consent_states: consent_states.clone(),
                }
            );

            let groups = provider.db().find_groups(GroupQueryArgs {
                conversation_type,
                consent_states,
                include_duplicate_dms: true,
                include_sync_groups: conversation_type
                    .map(|ct| matches!(ct, ConversationType::Sync))
                    .unwrap_or(true),
                ..Default::default()
            })?;

            let sync_groups = groups
                .iter()
                .filter_map(|g| match g {
                    StoredGroup {
                        conversation_type: ConversationType::Sync,
                        ..
                    } => Some(g.id.clone()),
                    _ => None,
                })
                .collect();
            let active_conversations = groups
                .into_iter()
                // TODO: Create find groups query only for group ID
                .map(|g| GroupId::from(g.id))
                .collect();

            Ok::<_, SubscribeError>((active_conversations, sync_groups))
        }
        .await?;

        let conversations =
            super::stream_conversations::StreamConversations::new(context, conversation_type)
                .await?;
        let messages = StreamGroupMessages::new(context, active_conversations).await?;

        Ok(Self {
            context,
            conversation_type,
            messages,
            conversations,
            sync_groups,
        })
    }
}

impl<'a, ApiClient, Db, Conversations> Stream
    for StreamAllMessages<
        'a,
        ApiClient,
        Db,
        Conversations,
        StreamGroupMessages<'a, ApiClient, Db, MessagesApiSubscription<'a, ApiClient>>,
    >
where
    ApiClient: XmtpApi + XmtpMlsStreams + 'a,
    Db: XmtpDb + 'a,
    Conversations: Stream<Item = Result<MlsGroup<ApiClient, Db>>>,
{
    type Item = Result<StoredGroupMessage>;

    #[tracing::instrument(skip_all, level = "trace", name = "poll_next_stream_all")]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use std::task::Poll::*;
        let mut this = self.as_mut().project();

        let next_message = this.messages.as_mut().poll_next(cx);
        if let Ready(Some(msg)) = next_message {
            if let Ok(msg) = &msg {
                if self.sync_groups.contains(&msg.group_id) {
                    let _ = self
                        .context
                        .worker_events()
                        .send(WorkerEvent::NewSyncGroupMsg);

                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
            }
            return Ready(Some(msg));
        }

        if let Ready(None) = next_message {
            return Ready(None);
        }

        if let Some(group) = ready!(this.conversations.poll_next(cx)) {
            this.messages.as_mut().add(group?);
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use crate::{assert_msg, builder::ClientBuilder};
    use futures::StreamExt;
    use std::sync::Arc;
    use std::time::Duration;
    use xmtp_mls_common::group::GroupMetadataOptions;

    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::associations::test_utils::WalletTestExt;

    #[rstest::rstest]
    #[xmtp_common::test]
    #[timeout(Duration::from_secs(20))]
    #[cfg_attr(target_arch = "wasm32", ignore)]
    async fn test_stream_all_messages_changing_group_list() {
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let caro_wallet = generate_local_wallet();
        let caro = ClientBuilder::new_test_client(&caro_wallet).await;

        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        tracing::info!("Created alix group {}", hex::encode(&alix_group.group_id));
        alix_group
            .add_members_by_inbox_id(&[caro.inbox_id()])
            .await
            .unwrap();

        let stream = caro.stream_all_messages(None, None).await.unwrap();
        futures::pin_mut!(stream);

        alix_group.send_message(b"first").await.unwrap();
        assert_msg!(stream, "first");
        let bo_group = bo
            .find_or_create_dm(caro_wallet.identifier(), None)
            .await
            .unwrap();

        bo_group.send_message(b"second").await.unwrap();
        assert_msg!(stream, "second");

        alix_group.send_message(b"third").await.unwrap();
        assert_msg!(stream, "third");

        let alix_group_2 = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_group_2
            .add_members_by_inbox_id(&[caro.inbox_id()])
            .await
            .unwrap();

        alix_group.send_message(b"fourth").await.unwrap();
        assert_msg!(stream, "fourth");

        alix_group_2.send_message(b"fifth").await.unwrap();
        assert_msg!(stream, "fifth");
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    #[timeout(Duration::from_secs(15))]
    async fn test_stream_all_messages_unchanging_group_list() {
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_group
            .add_members_by_inbox_id(&[caro.inbox_id()])
            .await
            .unwrap();

        let bo_group = bo
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        bo_group
            .add_members_by_inbox_id(&[caro.inbox_id()])
            .await
            .unwrap();

        let stream = caro.stream_all_messages(None, None).await.unwrap();
        futures::pin_mut!(stream);
        bo_group.send_message(b"first").await.unwrap();
        assert_msg!(stream, "first");

        bo_group.send_message(b"second").await.unwrap();
        assert_msg!(stream, "second");

        alix_group.send_message(b"third").await.unwrap();
        assert_msg!(stream, "third");

        bo_group.send_message(b"fourth").await.unwrap();
        assert_msg!(stream, "fourth");
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    #[timeout(Duration::from_secs(5))]
    async fn test_dm_stream_all_messages() {
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();

        let alix_dm = alix
            .find_or_create_dm_by_inbox_id(bo.inbox_id(), None)
            .await
            .unwrap();
        // TODO: This test does not work on web
        // unless these streams are in their own scope.
        // there's probably an issue with the old stream
        // not being dropped before the new stream starts.
        // Could be fixed by sending an abort signal to the JS stream.
        {
            // start a stream with only group messages
            let stream = bo
                .stream_all_messages(Some(ConversationType::Group), None)
                .await
                .unwrap();
            futures::pin_mut!(stream);
            alix_dm
                .send_message("first DM msg".as_bytes())
                .await
                .unwrap();
            alix_group
                .send_message("second GROUP msg".as_bytes())
                .await
                .unwrap();
            assert_msg!(stream, "second GROUP msg");
        }
        {
            // Start a stream with only dms
            let stream = bo
                .stream_all_messages(Some(ConversationType::Dm), None)
                .await
                .unwrap();
            futures::pin_mut!(stream);
            alix_group
                .send_message("second GROUP msg".as_bytes())
                .await
                .unwrap();
            alix_dm
                .send_message("second DM msg".as_bytes())
                .await
                .unwrap();
            assert_msg!(stream, "second DM msg");
        }
        // Start a stream with all conversations
        // Wait for 2 seconds for the group creation to be streamed
        let stream = bo.stream_all_messages(None, None).await.unwrap();
        futures::pin_mut!(stream);
        alix_group.send_message("first".as_bytes()).await.unwrap();
        assert_msg!(stream, "first");

        alix_dm.send_message("second".as_bytes()).await.unwrap();
        assert_msg!(stream, "second");
    }

    use std::collections::HashMap;
    fn find_duplicates_with_count(strings: &[String]) -> HashMap<&String, usize> {
        let mut counts = HashMap::new();

        // Count occurrences
        for string in strings {
            *counts.entry(string).or_insert(0) += 1;
        }

        // Filter to keep only strings that appear more than once
        counts.retain(|_, count| *count > 1);

        counts
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    #[timeout(Duration::from_secs(60))]
    #[cfg_attr(target_arch = "wasm32", ignore)]
    async fn test_stream_all_messages_does_not_lose_messages() {
        let mut replace = xmtp_common::TestLogReplace::default();
        let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let alix = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let eve = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bo = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        tracing::info!(inbox_id = eve.inbox_id(), installation_id = %eve.installation_id(), "EVE={}", eve.inbox_id());
        tracing::info!(inbox_id = bo.inbox_id(), installation_id = %bo.installation_id(), "BO={}", bo.inbox_id());
        tracing::info!(inbox_id = alix.inbox_id(), installation_id = %alix.installation_id(), "ALIX={}", alix.inbox_id());
        tracing::info!(inbox_id = caro.inbox_id(), installation_id = %caro.installation_id(), "CARO={}", caro.inbox_id());
        replace.add(caro.inbox_id(), "caro");
        replace.add(eve.inbox_id(), "eve");
        replace.add(alix.inbox_id(), "alix");
        replace.add(bo.inbox_id(), "bo");
        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_group
            .add_members_by_inbox_id(&[caro.inbox_id(), bo.inbox_id()])
            .await
            .unwrap();

        let bo_group = bo.sync_welcomes().await.unwrap()[0].clone();

        let mut stream = caro.stream_all_messages(None, None).await.unwrap();

        let alix_group_pointer = alix_group.clone();
        xmtp_common::spawn(None, async move {
            for i in 0..15 {
                let msg = format!("main spam {i}");
                alix_group_pointer
                    .send_message(msg.as_bytes())
                    .await
                    .unwrap();
                xmtp_common::time::sleep(Duration::from_micros(100)).await;
            }
        });

        // Eve will try to break our stream by creating lots of groups
        // and immediately sending a message
        // this forces our streams to re-subscribe
        let caro_id = caro.inbox_id().to_string();
        xmtp_common::spawn(None, async move {
            let caro = &caro_id;
            for i in 0..15 {
                let new_group = eve
                    .create_group(None, GroupMetadataOptions::default())
                    .unwrap();
                new_group.add_members_by_inbox_id(&[caro]).await.unwrap();
                let msg = format!("EVE spam {i} from new group");
                new_group.send_message(msg.as_bytes()).await.unwrap();
            }
        });

        // Bo will try to break our stream by sending lots of messages
        // this forces our streams to handle resubscribes while receiving lots of messages
        xmtp_common::spawn(None, async move {
            let bo_group = &bo_group;
            for i in 0..15 {
                bo_group
                    .send_message(format!("bo msg {i}").as_bytes())
                    .await
                    .unwrap();
                xmtp_common::time::sleep(Duration::from_millis(50)).await
            }
        });

        let mut messages = Vec::new();
        let timeout = if cfg!(target_arch = "wasm32") {
            Duration::from_secs(20)
        } else {
            Duration::from_secs(10)
        };
        loop {
            tokio::select! {
                Some(msg) = stream.next() => {
                    match msg {
                        Ok(m) => messages.push(m),
                        Err(e) => {
                            tracing::error!("error in stream test {e}");
                        }
                    }
                },
                _ = xmtp_common::time::sleep(timeout) => break
            }
        }

        let msgs = &messages
            .iter()
            .map(|m| String::from_utf8_lossy(m.decrypted_message_bytes.as_slice()).to_string())
            .collect::<Vec<String>>();
        let duplicates = find_duplicates_with_count(msgs);
        assert!(duplicates.is_empty());
        assert_eq!(messages.len(), 45, "too many messages mean duplicates, too little means missed. Also ensure timeout is sufficient.");
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    #[timeout(Duration::from_secs(10))]
    async fn test_stream_all_messages_detached_group_changes() {
        let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let hale = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let stream = caro.stream_all_messages(None, None).await.unwrap();

        let caro_id = caro.inbox_id().to_string();
        xmtp_common::spawn(None, async move {
            let caro = &caro_id;
            for i in 0..5 {
                let new_group = hale
                    .create_group(None, GroupMetadataOptions::default())
                    .unwrap();
                new_group.add_members_by_inbox_id(&[caro]).await.unwrap();
                tracing::info!(
                    "\n\n HALE SENDING {i} to group {}\n\n",
                    hex::encode(&new_group.group_id)
                );
                new_group
                    .send_message(b"spam from new group")
                    .await
                    .unwrap();
            }
        });

        let mut messages = Vec::new();
        let _ = xmtp_common::time::timeout(Duration::from_secs(20), async {
            futures::pin_mut!(stream);
            loop {
                if messages.len() < 5 {
                    if let Some(Ok(msg)) = stream.next().await {
                        tracing::info!(
                            message_id = hex::encode(&msg.id),
                            sender_inbox_id = msg.sender_inbox_id,
                            sender_installation_id = hex::encode(&msg.sender_installation_id),
                            group_id = hex::encode(&msg.group_id),
                            "GOT MESSAGE {}, text={}",
                            messages.len(),
                            String::from_utf8_lossy(msg.decrypted_message_bytes.as_slice())
                        );
                        messages.push(msg)
                    }
                } else {
                    break;
                }
            }
        })
        .await;
        tracing::info!("Total Messages: {}", messages.len());
        assert_eq!(messages.len(), 5);
    }

    #[rstest::rstest]
    #[case(ConsentState::Allowed, "msg in allowed")]
    #[case(ConsentState::Denied, "msg in denied")]
    #[case(ConsentState::Unknown, "msg in unknown")]
    #[xmtp_common::test]
    #[timeout(Duration::from_secs(20))]
    #[cfg_attr(target_arch = "wasm32", ignore)]
    async fn test_stream_all_messages_filters_by_consent_state(
        #[case] filter: ConsentState,
        #[case] expected_message: &str,
    ) {
        let sender = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let receiver = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // Create group with Allowed consent
        let allowed_group = sender
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        allowed_group
            .add_members_by_inbox_id(&[receiver.inbox_id()])
            .await
            .unwrap();

        // Create group with Denied consent
        let denied_group = sender
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        denied_group
            .add_members_by_inbox_id(&[receiver.inbox_id()])
            .await
            .unwrap();
        denied_group
            .update_consent_state(ConsentState::Denied)
            .unwrap();

        // Create group with Unknown consent
        let unknown_group = sender
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        unknown_group
            .add_members_by_inbox_id(&[receiver.inbox_id()])
            .await
            .unwrap();
        unknown_group
            .update_consent_state(ConsentState::Unknown)
            .unwrap();

        sender.sync_welcomes().await.unwrap();
        xmtp_common::time::sleep(Duration::from_millis(100)).await;

        let stream = sender
            .stream_all_messages(None, Some(vec![filter]))
            .await
            .unwrap();
        futures::pin_mut!(stream);

        allowed_group
            .send_message("msg in allowed".as_bytes())
            .await
            .unwrap();
        denied_group
            .send_message("msg in denied".as_bytes())
            .await
            .unwrap();
        unknown_group
            .send_message("msg in unknown".as_bytes())
            .await
            .unwrap();

        assert_msg!(stream, expected_message);
    }

    #[xmtp_common::test]
    async fn stream_messages_keeps_track_of_cursor() {
        let wallet = generate_local_wallet();
        let alice = Arc::new(ClientBuilder::new_test_client_no_sync(&wallet).await);
        let bob = ClientBuilder::new_test_client_no_sync(&generate_local_wallet()).await;
        let eve = ClientBuilder::new_test_client_no_sync(&generate_local_wallet()).await;
        let group = alice
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        group
            .add_members_by_inbox_id(&[bob.inbox_id(), eve.inbox_id()])
            .await
            .unwrap();
        let _bob_groups = bob.sync_welcomes().await.unwrap();
        let eve_groups = eve.sync_welcomes().await.unwrap();
        let eve_group = eve_groups.first().unwrap();
        group.sync().await.unwrap();
        // get the group epoch to 28
        for _ in 0..7 {
            group
                .update_group_name(format!("test name {}", xmtp_common::rand_string::<5>()))
                .await
                .unwrap();
        }
        for _ in 0..25 {
            eve_group
                .send_message(format!("message {}", xmtp_common::rand_string::<5>()).as_bytes())
                .await
                .unwrap();
        }
        // get the group epoch to 28
        for _ in 0..7 {
            group
                .update_group_name(format!("test name {}", xmtp_common::rand_string::<5>()))
                .await
                .unwrap();
        }
        group.sync().await.unwrap();
        // create a new installation for alice
        let alice_2 = ClientBuilder::new_test_client_no_sync(&wallet).await;
        let mut s = StreamAllMessages::new(&alice_2.context, None, None)
            .await
            .unwrap();
        // elapse enough time to update installations
        xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;
        group.update_installations().await.unwrap();
        // if the stream behaved as expected, it should have set the cursor to the latest
        // in the group before any messages that could actually be decrypted by alices
        // second installation were sent.

        // we should timeout because we have not gotten a decryptable message yet.
        let result = xmtp_common::time::timeout(std::time::Duration::from_secs(1), s.next()).await;
        assert!(matches!(result.unwrap_err(), xmtp_common::time::Expired));

        {
            let msg_stream = &s.messages;
            let cursor = msg_stream
                .group_list
                .get(group.group_id.as_slice())
                .unwrap();
            assert!(cursor.pos() > 1);
        }

        eve_group
            .send_message(b"decryptable message")
            .await
            .unwrap();
        assert_msg!(s, "decryptable message");
    }
}
