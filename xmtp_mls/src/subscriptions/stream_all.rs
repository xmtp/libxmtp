use std::{
    pin::Pin,
    task::{ready, Context, Poll},
};

use crate::subscriptions::stream_messages::MessagesApiSubscription;
use crate::{
    groups::{scoped_client::ScopedGroupClient, MlsGroup},
    Client,
};

use futures::stream::Stream;
use xmtp_db::{
    group::{ConversationType, GroupQueryArgs},
    group_message::StoredGroupMessage,
};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::api_client::{trait_impls::XmtpApi, XmtpMlsStreams};

use super::{
    stream_conversations::{StreamConversations, WelcomesApiSubscription},
    stream_messages::StreamGroupMessages,
    Result, SubscribeError,
};
use xmtp_common::types::GroupId;

use pin_project_lite::pin_project;

pin_project! {
    pub(super) struct StreamAllMessages<'a, C, Conversations, Messages> {
        #[pin] conversations: Conversations,
        #[pin] messages: Messages,
        client: &'a C,
        conversation_type: Option<ConversationType>,
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
    ) -> Result<Self> {
        let active_conversations = async {
            let provider = client.mls_provider()?;
            client.sync_welcomes(&provider).await?;

            let active_conversations = provider
                .conn_ref()
                .find_groups(GroupQueryArgs::default().maybe_conversation_type(conversation_type).include_duplicate_dms(true))?
                .into_iter()
                // TODO: Create find groups query only for group ID
                .map(|g| GroupId::from(g.id))
                .collect();
            Ok::<_, SubscribeError>(active_conversations)
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

        let stream = caro.stream_all_messages(None).await.unwrap();
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

        let stream = caro.stream_all_messages(None).await.unwrap();
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
                .stream_all_messages(Some(ConversationType::Group))
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
                .stream_all_messages(Some(ConversationType::Dm))
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
        let stream = bo.stream_all_messages(None).await.unwrap();
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

        let mut stream = caro.stream_all_messages(None).await.unwrap();

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
    #[timeout(Duration::from_secs(5))]
    async fn test_stream_all_messages_detached_group_changes() {
        let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let hale = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let stream = caro.stream_all_messages(None).await.unwrap();

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
}
