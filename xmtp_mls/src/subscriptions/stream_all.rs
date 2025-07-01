#[cfg(test)]
mod tests;

use std::{
    borrow::Cow,
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
};

use crate::{
    context::{XmtpContextProvider, XmtpMlsLocalContext},
    subscriptions::stream_messages::MessagesApiSubscription,
};
use crate::{groups::welcome_sync::WelcomeService, track};

use xmtp_db::{
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
use crate::subscriptions::SyncWorkerEvent;
use futures::stream::Stream;
use xmtp_common::types::GroupId;
use xmtp_db::{consent_record::ConsentState, group::StoredGroup};

use pin_project_lite::pin_project;

pin_project! {
    pub(super) struct StreamAllMessages<'a, ApiClient, Db, Conversations, Messages> {
        #[pin] conversations: Conversations,
        #[pin] messages: Messages,
        context: Cow<'a, Arc<XmtpMlsLocalContext<ApiClient, Db>>>,
        conversation_type: Option<ConversationType>,
        sync_groups: Vec<Vec<u8>>
    }
}

impl<A, D>
    StreamAllMessages<
        'static,
        A,
        D,
        StreamConversations<'static, A, D, WelcomesApiSubscription<'static, A>>,
        StreamGroupMessages<'static, A, D, MessagesApiSubscription<'static, A>>,
    >
where
    A: XmtpApi + XmtpMlsStreams + Send + Sync + 'static,
    D: XmtpDb + Send + Sync + 'static,
{
    pub async fn new_owned(
        context: Arc<XmtpMlsLocalContext<A, D>>,
        conversation_type: Option<ConversationType>,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<Self> {
        Self::from_cow(Cow::Owned(context), conversation_type, consent_states).await
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
        Self::from_cow(Cow::Borrowed(context), conversation_type, consent_states).await
    }

    pub async fn from_cow(
        context: Cow<'a, Arc<XmtpMlsLocalContext<A, D>>>,
        conversation_type: Option<ConversationType>,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<Self> {
        let (active_conversations, sync_groups) = async {
            let provider = context.mls_provider();
            WelcomeService::new(context.as_ref().clone())
                .sync_welcomes()
                .await?;

            track!(
                "Message Stream Connect",
                {
                    "conversation_type": conversation_type,
                    "consent_states": &consent_states,
                },
                icon: "🚣"
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

        let conversations = super::stream_conversations::StreamConversations::from_cow(
            context.clone(),
            conversation_type,
        )
        .await?;
        let messages = StreamGroupMessages::from_cow(context.clone(), active_conversations).await?;

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
                        .send(SyncWorkerEvent::NewSyncGroupMsg);
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
