use std::{
    collections::HashMap,
    pin::Pin,
    task::{Context, Poll},
};

use crate::subscriptions::stream_messages::{MessagePositionCursor, MessagesApiSubscription};
use crate::{
    groups::{scoped_client::ScopedGroupClient, MlsGroup},
    storage::{
        group::{ConversationType, GroupQueryArgs},
        group_message::StoredGroupMessage,
    },
    Client,
};
use futures::{stream::Stream, Future};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::api_client::{trait_impls::XmtpApi, XmtpMlsStreams};

use super::{
    stream_conversations::{StreamConversations, WelcomesApiSubscription},
    stream_messages::StreamGroupMessages,
    FutureWrapper, Result, SubscribeError,
};
use pin_project_lite::pin_project;

pin_project! {
    pub(super) struct StreamAllMessages<'a, C, Conversations, Messages> {
        #[pin] conversations: Conversations,
        #[pin] messages: Messages,
        #[pin] state: SwitchState<'a, Messages>,
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
                .find_groups(GroupQueryArgs::default().maybe_conversation_type(conversation_type))?
                .into_iter()
                .map(Into::into)
                .collect::<HashMap<Vec<u8>, MessagePositionCursor>>();
            Ok::<_, SubscribeError>(active_conversations)
        }
        .await?;

        let messages = StreamGroupMessages::new(client, active_conversations).await?;
        let conversations = super::stream_conversations::StreamConversations::new(
            client,
            conversation_type.clone(),
        )
        .await?;

        Ok(Self {
            client,
            conversation_type,
            messages,
            conversations,
            state: Default::default(),
        })
    }
}

pin_project! {
    #[project = SwitchProject]
    #[derive(Default)]
    enum SwitchState<'a, Out> {
        /// State that indicates the stream is waiting on the next message from the network
        #[default]
        Waiting,
        /// state that indicates the stream is waiting on a IO/Network future to finish processing
        /// the current message before moving on to the next one
        Switching {
            #[pin] future: FutureWrapper<'a, Result<Out>>
        }
    }
}

impl<Out> std::fmt::Debug for SwitchState<'_, Out> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SwitchState::Waiting => write!(f, "waiting"),
            SwitchState::Switching { .. } => write!(f, "switching"),
        }
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
        // tracing::debug!("POLLING STREAM ALL");
        use std::task::Poll::*;
        use SwitchProject::*;
        let this = self.as_mut().project();

        let state = this.state.project();
        match state {
            Waiting => {
                if let Ready(msg) = this.messages.poll_next(cx) {
                    return Ready(msg);
                }
                if let Ready(Some(Ok(group))) = this.conversations.poll_next(cx) {
                    self.as_mut().begin_switch_stream(group);
                    tracing::trace!("stream all state = {:?}", self.state);
                }
                cx.waker().wake_by_ref();
                Pending
            }
            // switching message streams
            Switching { future } => match future.poll(cx) {
                Ready(Ok(stream)) => {
                    self.as_mut().end_switch_stream(stream, cx);
                    tracing::trace!("stream all state state = {:?}", self.state);
                    Pending
                }
                Ready(Err(e)) => {
                    tracing::error!("Error swapping message stream in StreamAllMessages {}", e);
                    Ready(Some(Err(e)))
                }
                Pending => {
                    cx.waker().wake_by_ref();
                    Pending
                }
            },
        }
    }
}

impl<'a, C, Conversations>
    StreamAllMessages<
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
    /// Polls groups
    /// if groups are available, the stream starts waiting for the future to switch message
    /// streams.
    fn begin_switch_stream(mut self: Pin<&mut Self>, new_group: MlsGroup<C>) {
        if self.messages.group_list().contains_key(&new_group.group_id) {
            return;
        }

        tracing::trace!(
            inbox_id = self.client.inbox_id(),
            installation_id = %self.client.installation_id(),
            group_id = hex::encode(&new_group.group_id),
            "begin establishing new message stream to include group_id={}",
            hex::encode(&new_group.group_id)
        );

        let mut conversations = self.messages.group_list().clone();
        conversations.insert(new_group.group_id, 1.into());

        let future = StreamGroupMessages::new(self.client, conversations);
        let mut this = self.as_mut().project();
        this.state.set(SwitchState::Switching {
            future: FutureWrapper::new(future),
        });
    }

    fn end_switch_stream(
        mut self: Pin<&mut Self>,
        stream: StreamGroupMessages<'a, C, MessagesApiSubscription<'a, C>>,
        cx: &mut Context<'_>,
    ) {
        let mut this = self.as_mut().project();
        // drain the stream
        // if we don't drain the stream, we inadvertantly create a zombie stream
        // that freezes the executor
        // Not entirely certain why it happens, but i assume gRPC does not like closing the stream
        // because we have unread items in queue.
        // We can throw away the drained messages, because we set the cursor for the stream
        // before these messages were received
        this.messages.as_mut().drain(cx);
        this.messages.set(stream);
        this.state.as_mut().set(SwitchState::Waiting);
        // TODO: take old group list and .diff with new, to check which group is new
        // for log msg.
        tracing::trace!("established new stream");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use std::sync::Arc;

    use crate::{assert_msg, builder::ClientBuilder, groups::GroupMetadataOptions};
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::InboxOwner;

    use futures::StreamExt;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread", worker_threads = 10))]
    async fn test_stream_all_messages_changing_group_list() {
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let caro_wallet = generate_local_wallet();
        let caro = ClientBuilder::new_test_client(&caro_wallet).await;

        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_group
            .add_members_by_inbox_id(&[caro.inbox_id()])
            .await
            .unwrap();

        let stream = caro.stream_all_messages(None).await.unwrap();
        futures::pin_mut!(stream);

        alix_group.send_message(b"first").await.unwrap();
        assert_msg!(stream, "first");
        let bo_group = bo.create_dm(caro_wallet.get_address()).await.unwrap();

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

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread", worker_threads = 10))]
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

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread"))]
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
            .create_dm_by_inbox_id(&alix.mls_provider().unwrap(), bo.inbox_id().to_string())
            .await
            .unwrap();

        // start a stream with only group messages
        let stream = bo
            .stream_all_messages(Some(ConversationType::Group))
            .await
            .unwrap();
        futures::pin_mut!(stream);
        alix_dm.send_message("first".as_bytes()).await.unwrap();
        alix_group.send_message("second".as_bytes()).await.unwrap();
        assert_msg!(stream, "second");

        // Start a stream with only dms
        // Wait for 2 seconds for the group creation to be streamed
        let stream = bo
            .stream_all_messages(Some(ConversationType::Dm))
            .await
            .unwrap();
        futures::pin_mut!(stream);
        alix_group.send_message("first".as_bytes()).await.unwrap();
        alix_dm.send_message("second".as_bytes()).await.unwrap();
        assert_msg!(stream, "second");

        // Start a stream with all conversations
        // Wait for 2 seconds for the group creation to be streamed
        let stream = bo.stream_all_messages(None).await.unwrap();
        futures::pin_mut!(stream);
        alix_group.send_message("first".as_bytes()).await.unwrap();
        assert_msg!(stream, "first");

        alix_dm.send_message("second".as_bytes()).await.unwrap();
        assert_msg!(stream, "second");
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread", worker_threads = 10))]
    async fn test_stream_all_messages_does_not_lose_messages() {
        let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let alix = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let eve = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        tracing::info!(inbox_id = eve.inbox_id(), installation_id = %eve.installation_id(), "EVE");

        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_group
            .add_members_by_inbox_id(&[caro.inbox_id()])
            .await
            .unwrap();

        let stream = caro.stream_all_messages(None).await.unwrap();

        let alix_group_pointer = alix_group.clone();
        crate::spawn(None, async move {
            let mut sent = 0;
            for _ in 0..50 {
                alix_group_pointer.send_message(b"spam").await.unwrap();
                sent += 1;
                xmtp_common::time::sleep(core::time::Duration::from_micros(100)).await;
                tracing::info!("sent {sent}");
            }
        });

        // Eve will try to break our stream by creating lots of groups
        // and immediately sending a message
        // this forces our streams to re-subscribe
        let caro_id = caro.inbox_id().to_string();
        crate::spawn(None, async move {
            let caro = &caro_id;
            for i in 0..50 {
                let new_group = eve
                    .create_group(None, GroupMetadataOptions::default())
                    .unwrap();
                new_group.add_members_by_inbox_id(&[caro]).await.unwrap();
                tracing::info!("\n\n EVE SENDING {i} \n\n");
                new_group
                    .send_message(b"spam from new group")
                    .await
                    .unwrap();
            }
        });

        let mut messages = Vec::new();
        let _ = tokio::time::timeout(core::time::Duration::from_secs(60), async {
            futures::pin_mut!(stream);
            loop {
                if messages.len() < 100 {
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
        assert_eq!(messages.len(), 100);
    }
}
