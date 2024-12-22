use std::{
    collections::HashSet, future::Future, marker::PhantomData, pin::Pin,
    sync::Arc, task::Poll,
};

use crate::{
    groups::{scoped_client::ScopedGroupClient, MlsGroup},
    storage::{group::ConversationType, DbConnection, NotFound},
    Client, XmtpOpenMlsProvider,
};
use futures::{future::FutureExt, prelude::stream::Select, Stream};
use pin_project_lite::pin_project;
use tokio_stream::wrappers::BroadcastStream;
use xmtp_common::{retry_async, Retry};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::{
    api_client::{trait_impls::XmtpApi, XmtpMlsStreams},
    xmtp::mls::api::v1::{welcome_message::V1 as WelcomeMessageV1, WelcomeMessage},
};

use super::{temp::Result, LocalEvents, SubscribeError};

enum WelcomeOrGroup<C> {
    Group(Result<MlsGroup<C>>),
    Welcome(Result<WelcomeMessage>),
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
    S: Stream<Item = std::result::Result<WelcomeMessage, xmtp_proto::Error>>,
{
    type Item = WelcomeOrGroup<C>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        use std::task::Poll::*;
        let this = self.project();

        match this.inner.poll_next(cx) {
            Ready(Some(welcome)) => {
                let welcome = welcome.map_err(SubscribeError::from);
                Ready(Some(WelcomeOrGroup::Welcome(welcome)))
            }
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
        known_welcome_ids: HashSet<i64>,
        #[pin] state: ProcessState<'a, C>,
    }
}

pin_project! {
    #[project = ProcessProject]
    enum ProcessState<'a, C> {
        /// State where we are waiting on the next Message from the network
        Waiting,
        /// State where we are waiting on an IO/Network future to finish processing the current message
        /// before moving on to the next one
        Processing {
            #[pin] future: Pin<Box<dyn Future<Output = Result< (MlsGroup<C>, Option<i64>) >> + 'a >>
        }
    }
}

impl<'a, C> Default for ProcessState<'a, C> {
    fn default() -> Self {
        ProcessState::Waiting
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
    ) -> Result<Self> {
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
            state: ProcessState::Waiting,
        })
    }
}

impl<'a, C, Subscription> Stream for StreamConversations<'a, C, Subscription>
where
    C: ScopedGroupClient + Clone,
    Subscription: Stream<Item = Result<WelcomeOrGroup<C>>> + 'a,
{
    type Item = Result<MlsGroup<C>>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll::*;
        use ProcessState::*;
        let mut this = self.as_mut().project();

        match this.state.as_mut().project() {
            ProcessProject::Waiting => {
                match this.inner.poll_next(cx) {
                    Ready(Some(item)) => {
                        let future =
                            // need to clone client into Arc<> here b/c:
                            // otherwise the `'1` ref for `Pin<&mut Self>` in arg to `poll_next` needs to
                            // live as long as `'a` ref for `Client`.
                            // This is because we're boxing this future (i.e `Box<dyn Future + 'a>`).
                            // There maybe a way to avoid it, but we need to `Box<>` the type
                            // b/c there's no way to get the anonymous future type on the stack generated by an
                            // `async fn`. If we can somehow store `impl Trait` on a struct (or
                            // something similar), we could avoid the `Clone` + `Arc`ing.
                            Self::process_new_item(this.known_welcome_ids.clone(), Arc::new(this.client.clone()), item);

                        this.state.set(ProcessState::Processing {
                            future: future.boxed(),
                        });
                        Pending
                    }
                    // stream ended
                    Ready(None) => Ready(None),
                    Pending => {
                        cx.waker().wake_by_ref();
                        Pending
                    }
                }
            }
            /// We're processing a message we received
            ProcessProject::Processing { future } => match future.poll(cx) {
                Ready(Ok((group, welcome_id))) => {
                    if let Some(id) = welcome_id {
                        this.known_welcome_ids.insert(id);
                    }
                    this.state.set(ProcessState::Waiting);
                    Ready(Some(Ok(group)))
                }
                Ready(Err(e)) => Ready(Some(Err(e))),
                Pending => Pending,
            },
        }
    }
}

impl<'a, C, Subscription> StreamConversations<'a, C, Subscription>
where
    C: ScopedGroupClient + Clone,
{
    async fn process_new_item(
        known_welcome_ids: HashSet<i64>,
        client: Arc<C>,
        item: Result<WelcomeOrGroup<C>>,
    ) -> Result<(MlsGroup<C>, Option<i64>)> {
        use WelcomeOrGroup::*;
        let provider = client.context().mls_provider()?;
        match item? {
            Welcome(w) => Self::on_welcome(&known_welcome_ids, client, &provider, w?).await,
            Group(g) => {
                todo!()
            }
        }
    }

    // process a new welcome, returning the new welcome ID
    async fn on_welcome(
        known_welcome_ids: &HashSet<i64>,
        client: Arc<C>,
        provider: &XmtpOpenMlsProvider,
        welcome: WelcomeMessage,
    ) -> Result<(MlsGroup<C>, Option<i64>)> {
        let WelcomeMessageV1 {
            id,
            ref created_ns,
            ref installation_key,
            ref data,
            ref hpke_public_key,
        } = crate::client::extract_welcome_message(welcome)?;
        let id = id as i64;

        if known_welcome_ids.contains(&(id)) {
            let conn = provider.conn_ref();
            let group = conn
                .find_group_by_welcome_id(id)?
                .ok_or(NotFound::GroupByWelcome(id))?;
            tracing::info!(
                inbox_id = client.inbox_id(),
                group_id = hex::encode(&group.id),
                welcome_id = ?group.welcome_id,
                "Loading existing group for welcome_id: {:?}",
                group.welcome_id
            );
            return Ok((
                MlsGroup::new(Arc::unwrap_or_clone(client), group.id, group.created_at_ns),
                Some(id),
            ));
        }

        let c = &client;
        let mls_group = retry_async!(
            Retry::default(),
            (async {
                tracing::info!(
                    installation_id = hex::encode(installation_key),
                    welcome_id = &id,
                    "Trying to process streamed welcome"
                );

                (*client)
                    .context()
                    .store()
                    .transaction_async(provider, |provider| async move {
                        MlsGroup::create_from_encrypted_welcome(
                            Arc::clone(c),
                            provider,
                            hpke_public_key.as_slice(),
                            data,
                            id,
                        )
                        .await
                    })
                    .await
            })
        )?;

        Ok((mls_group, Some(id)))
    }
}
