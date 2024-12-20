use std::{collections::HashSet, marker::PhantomData, sync::Arc, task::Poll};

use futures::{prelude::stream::Select, Stream};
use pin_project_lite::pin_project;
use tokio_stream::wrappers::BroadcastStream;
use xmtp_common::{retry_async, Retry};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::{
    api_client::{trait_impls::XmtpApi, XmtpMlsStreams},
    xmtp::mls::api::v1::WelcomeMessage,
};

use crate::{
    groups::{scoped_client::ScopedGroupClient, MlsGroup},
    storage::{group::ConversationType, DbConnection},
    Client, XmtpOpenMlsProvider,
};

use super::{LocalEvents, SubscribeError};

enum WelcomeOrGroup<C> {
    Group(Result<MlsGroup<C>, SubscribeError>),
    Welcome(Result<WelcomeMessage, xmtp_proto::Error>),
}

pin_project! {
    /// Broadcast stream filtered + mapped to WelcomeOrGroup
    struct BroadcastGroupStream<C> {
        #[pin] inner: BroadcastStream<LocalEvents<C>>,
    }
}

impl<C> BroadcastGroupStream<C> {
    fn new(inner: BroadcastStream<LocalEvents<C>>) -> Self {
        Self { inner }
    }
}

impl<C> Stream for BroadcastGroupStream<C>
where
    C: Clone + Send + Sync + 'static, // required by tokio::BroadcastStream
{
    type Item = WelcomeOrGroup<C>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        use std::task::Poll::*;
        let this = self.project();

        match this.inner.poll_next(cx) {
            Ready(Some(event)) => {
                let ev = xmtp_common::optify!(event, "Missed messages due to event queue lag")
                    .and_then(LocalEvents::group_filter);
                if let Some(g) = ev {
                    Ready(Some(WelcomeOrGroup::<C>::Group(Ok(g))))
                } else {
                    // skip this item since it was either missed due to lag, or not a group
                    Pending
                }
            }
            Pending => Pending,
            Ready(None) => Ready(None),
        }
    }
}

pin_project! {
    /// Subscription Stream mapped to WelcomeOrGroup
    struct SubscriptionStream<S, C> {
        #[pin] inner: S,
        _marker: PhantomData<C>,
    }
}

impl<S, C> SubscriptionStream<S, C> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            _marker: PhantomData,
        }
    }
}

impl<S, C> Stream for SubscriptionStream<S, C>
where
    S: Stream<Item = Result<WelcomeMessage, xmtp_proto::Error>>,
{
    type Item = WelcomeOrGroup<C>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        use std::task::Poll::*;
        let this = self.project();

        match this.inner.poll_next(cx) {
            Ready(Some(welcome)) => Ready(Some(WelcomeOrGroup::Welcome(welcome))),
            Pending => Pending,
            Ready(None) => Ready(None),
        }
    }
}

pin_project! {
    pub struct StreamConversations<'a, C, Subscription> {
        client: &'a C,
        #[pin] inner: Subscription,
        conversation_type: Option<ConversationType>,
        known_welcome_ids: HashSet<i64>
    }
}

type MultiplexedSelect<C, S> = Select<BroadcastGroupStream<C>, SubscriptionStream<S, C>>;

impl<'a, A, V>
    StreamConversations<
        'a,
        Client<A, V>,
        MultiplexedSelect<Client<A, V>, <A as XmtpMlsStreams>::WelcomeMessageStream<'a>>,
    >
where
    A: XmtpApi + XmtpMlsStreams + Send + Sync + 'static,
    V: SmartContractSignatureVerifier + Send + Sync + 'static,
{
    pub async fn new(
        client: &'a Client<A, V>,
        conversation_type: Option<ConversationType>,
        conn: &DbConnection,
    ) -> Result<Self, SubscribeError> {
        let installation_key = client.installation_public_key();
        let id_cursor = 0;
        tracing::info!(
            inbox_id = client.inbox_id(),
            "Setting up conversation stream"
        );

        let events =
            BroadcastGroupStream::new(BroadcastStream::new(client.local_events.subscribe()));

        let subscription = client
            .api_client
            .subscribe_welcome_messages(installation_key.as_ref(), Some(id_cursor))
            .await?;
        let subscription = SubscriptionStream::new(subscription);
        let known_welcome_ids = HashSet::from_iter(conn.group_welcome_ids()?.into_iter());

        let stream = futures::stream::select(events, subscription);

        Ok(Self {
            client,
            inner: stream,
            known_welcome_ids,
            conversation_type,
        })
    }
}

impl<'a, C, Subscription> Stream for StreamConversations<'a, C, Subscription>
where
    C: ScopedGroupClient + Clone,
    Subscription: Stream<Item = Result<WelcomeOrGroup<C>, SubscribeError>>,
{
    type Item = Result<MlsGroup<C>, SubscribeError>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll::*;
        let this = self.project();

        match this.inner.poll_next(cx) {
            Ready(Some(msg)) => {
                todo!()
            }
            // stream ended
            Ready(None) => Ready(None),
            Pending => {
                cx.waker().wake_by_ref();
                Pending
            }
        }
    }
}

impl<'a, C, Subscription> StreamConversations<'a, C, Subscription>
where
    C: ScopedGroupClient + Clone,
{
    async fn process_streamed_welcome(
        &mut self,
        client: C,
        provider: &XmtpOpenMlsProvider,
        welcome: WelcomeMessage,
    ) -> Result<MlsGroup<C>, SubscribeError> {
        let welcome_v1 = crate::client::extract_welcome_message(welcome)?;
        if self.known_welcome_ids.contains(&(welcome_v1.id as i64)) {
            let conn = provider.conn_ref();
            self.known_welcome_ids.insert(welcome_v1.id as i64);
            let group = conn.find_group_by_welcome_id(welcome_v1.id as i64)?;
            tracing::info!(
                inbox_id = client.inbox_id(),
                group_id = hex::encode(&group.id),
                welcome_id = ?group.welcome_id,
                "Loading existing group for welcome_id: {:?}",
                group.welcome_id
            );
            return Ok(MlsGroup::new(client.clone(), group.id, group.created_at_ns));
        }

        let creation_result = retry_async!(
            Retry::default(),
            (async {
                tracing::info!(
                    installation_id = &welcome_v1.id,
                    "Trying to process streamed welcome"
                );
                let welcome_v1 = &welcome_v1;
                client
                    .context
                    .store()
                    .transaction_async(provider, |provider| async move {
                        MlsGroup::create_from_encrypted_welcome(
                            Arc::new(client.clone()),
                            provider,
                            welcome_v1.hpke_public_key.as_slice(),
                            &welcome_v1.data,
                            welcome_v1.id as i64,
                        )
                        .await
                    })
                    .await
            })
        );

        Ok(creation_result?)
    }
}
