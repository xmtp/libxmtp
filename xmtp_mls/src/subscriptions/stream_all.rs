use super::{
    stream_conversations::{StreamConversations, WelcomesApiSubscription},
    stream_messages::StreamGroupMessages,
    Result, SubscribeError,
};
use crate::subscriptions::{
    stream_messages::MessagesApiSubscription, LocalEvents, SyncWorkerEvent,
};
use crate::{
    groups::{scoped_client::ScopedGroupClient, MlsGroup},
    Client,
};
use futures::stream::Stream;
use std::{
    pin::Pin,
    task::{ready, Context, Poll},
};
use xmtp_common::types::GroupId;
use xmtp_db::{
    consent_record::ConsentState,
    group::{ConversationType, GroupQueryArgs, StoredGroup},
    group_message::StoredGroupMessage,
};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::api_client::{trait_impls::XmtpApi, XmtpMlsStreams};

use pin_project_lite::pin_project;

pin_project! {
    pub(super) struct StreamAllMessages<'a, C, Conversations, Messages> {
        #[pin] conversations: Conversations,
        #[pin] messages: Messages,
        client: &'a C,
        conversation_type: Option<ConversationType>,
        sync_groups: Vec<Vec<u8>>
    }
}

impl<'a, A, V>
    StreamAllMessages<
        'a,
        Client<A, V>,
        StreamConversations<'a, Client<A, V>, WelcomesApiSubscription<'a, Client<A, V>>>,
        StreamGroupMessages<'a, Client<A, V>, MessagesApiSubscription<'a, Client<A, V>>>,
    >
where
    A: XmtpApi + XmtpMlsStreams + Send + Sync + 'static,
    V: SmartContractSignatureVerifier + Send + Sync + 'static,
{
    pub async fn new(
        client: &'a Client<A, V>,
        conversation_type: Option<ConversationType>,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<Self> {
        let (active_conversations, sync_groups) = async {
            let provider = client.mls_provider()?;
            client.sync_welcomes(&provider).await?;

            let groups = provider.conn_ref().find_groups(GroupQueryArgs {
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
            super::stream_conversations::StreamConversations::new(client, conversation_type)
                .await?;
        let messages = StreamGroupMessages::new(client, active_conversations).await?;

        Ok(Self {
            client,
            conversation_type,
            messages,
            conversations,
            sync_groups,
        })
    }
}

impl<'a, C, Conversations> Stream
    for StreamAllMessages<
        'a,
        C,
        Conversations,
        StreamGroupMessages<'a, C, MessagesApiSubscription<'a, C>>,
    >
where
    C: ScopedGroupClient + Clone + 'a,
    <C as ScopedGroupClient>::ApiClient: XmtpApi + XmtpMlsStreams + 'a,
    Conversations: Stream<Item = Result<MlsGroup<C>>>,
{
    type Item = Result<StoredGroupMessage>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use std::task::Poll::*;
        let mut this = self.as_mut().project();

        if let Ready(msg) = this.messages.as_mut().poll_next(cx) {
            if let Some(Ok(msg)) = &msg {
                if self.sync_groups.contains(&msg.group_id) {
                    let _ = self
                        .client
                        .local_events()
                        .send(LocalEvents::SyncWorkerEvent(
                            SyncWorkerEvent::NewSyncGroupMsg,
                        ));
                    return self.poll_next(cx);
                }
            };

            return Ready(msg);
        }
        if let Some(group) = ready!(this.conversations.poll_next(cx)) {
            this.messages.as_mut().add(group?);
            return self.poll_next(cx);
        }
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use crate::groups::DMMetadataOptions;
    use crate::{assert_msg, builder::ClientBuilder, groups::GroupMetadataOptions};
    use futures::StreamExt;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::sleep;

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
            .find_or_create_dm(caro_wallet.identifier(), DMMetadataOptions::default())
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
            .find_or_create_dm_by_inbox_id(bo.inbox_id().to_string(), DMMetadataOptions::default())
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
    #[timeout(Duration::from_secs(15))]
    #[cfg_attr(target_arch = "wasm32", ignore)]
    async fn test_stream_all_messages_does_not_lose_messages() {
        let mut replace = xmtp_common::InboxIdReplace::default();
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

        let provider = bo.store().mls_provider().unwrap();
        let bo_group = bo.sync_welcomes(&provider).await.unwrap()[0].clone();

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
        let timeout = Duration::from_secs(10);
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
        /*
        for message in messages.iter() {
            let m = String::from_utf8_lossy(message.decrypted_message_bytes.as_slice());
            tracing::info!("{}", m);
        }*/
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
    #[ignore]
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

        let provider = sender.mls_provider().unwrap();
        sender.sync_welcomes(&provider).await.unwrap();
        sleep(Duration::from_millis(100)).await;

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
        let _bob_groups = bob
            .sync_welcomes(&bob.mls_provider().unwrap())
            .await
            .unwrap();
        let eve_groups = eve
            .sync_welcomes(&eve.mls_provider().unwrap())
            .await
            .unwrap();
        let eve_group = eve_groups.first().unwrap();
        group.sync().await.unwrap();
        // get the group epoch to 28
        for _ in 0..14 {
            group
                .update_group_name(format!("test name {}", xmtp_common::rand_string::<5>()))
                .await
                .unwrap();
        }
        for _ in 0..100 {
            eve_group
                .send_message(format!("message {}", xmtp_common::rand_string::<5>()).as_bytes())
                .await
                .unwrap();
        }
        // get the group epoch to 28
        for _ in 0..14 {
            group
                .update_group_name(format!("test name {}", xmtp_common::rand_string::<5>()))
                .await
                .unwrap();
        }
        group.sync().await.unwrap();
        // create a new installation for alice
        let alice_2 = ClientBuilder::new_test_client_no_sync(&wallet).await;
        let mut s = StreamAllMessages::new(&alice_2, None, None).await.unwrap();
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
            assert!(*cursor > 1.into());
        }

        eve_group
            .send_message(b"decryptable message")
            .await
            .unwrap();
        assert_msg!(s, "decryptable message");
    }
}
