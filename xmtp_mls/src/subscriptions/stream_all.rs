use std::{collections::HashMap, sync::Arc};

use crate::{
    client::ClientError,
    groups::scoped_client::ScopedGroupClient,
    groups::subscriptions,
    storage::{
        group::{ConversationType, GroupQueryArgs},
        group_message::StoredGroupMessage,
    },
    Client,
};
use futures::{
    stream::{self, Stream, StreamExt},
    Future,
};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::api_client::{trait_impls::XmtpApi, XmtpMlsStreams};

use super::{
    stream_conversations::{StreamConversations, WelcomesApiSubscription},
    FutureWrapper, MessagesStreamInfo, SubscribeError,
};
pub struct StreamAllMessages<'a, C, Welcomes, Messages> {
    /// The monolithic XMTP Client
    client: &'a C,
    /// Type of conversation to stream
    conversation_type: Option<ConversationType>,
    /// Conversations that are being actively streamed
    active_conversations: HashMap<Vec<u8>, MessagesStreamInfo>,
    /// Welcomes Stream
    welcomes: Welcomes,
    /// Messages Stream
    messages: Messages,
    /// Extra messages from message stream, when the stream switches because
    /// of a new group received.
    extra_messages: Vec<StoredGroupMessage>,
}

impl<'a, A, V, Messages>
    StreamAllMessages<
        'a,
        Client<A, V>,
        StreamConversations<'a, Client<A, V>, WelcomesApiSubscription<'a, A>>,
        FutureWrapper<'a, Result<StoredGroupMessage, SubscribeError>>,
    >
where
    A: XmtpApi + XmtpMlsStreams + Send + Sync + 'static,
    V: SmartContractSignatureVerifier + Send + Sync + 'static,
{
    pub async fn new(
        client: &'a Client<A, V>,
        conversation_type: Option<ConversationType>,
    ) -> Result<Self, SubscribeError> {
        let mut active_conversations = async {
            let provider = client.mls_provider()?;
            client.sync_welcomes(&provider).await?;

            let active_conversations = provider
                .conn_ref()
                .find_groups(GroupQueryArgs::default().maybe_conversation_type(conversation_type))?
                .into_iter()
                .map(Into::into)
                .collect::<HashMap<Vec<u8>, MessagesStreamInfo>>();
            Ok::<_, ClientError>(active_conversations)
        }
        .await?;

        let messages =
            subscriptions::stream_messages(client, Arc::new(active_conversations.clone())).await?;
        let messages = FutureWrapper::new(messages);
        let welcomes = super::stream_conversations::StreamConversations::new(
            client,
            conversation_type.clone(),
        )
        .await?;

        Ok(Self {
            client,
            conversation_type,
            messages,
            welcomes,
            active_conversations,
            extra_messages: Vec::new(),
        })
    }
}

impl<'a, C, Welcomes, Messages> Stream for StreamAllMessages<'a, C, Welcomes, Messages>
where
    C: ScopedGroupClient,
{
    type Item = Result<StoredGroupMessage, SubscribeError>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        todo!()
    }
}
