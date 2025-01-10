use std::{
    collections::HashSet,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use crate::{
    groups::{scoped_client::ScopedGroupClient, MlsGroup},
    storage::{group::ConversationType, NotFound},
    Client, XmtpOpenMlsProvider,
};
use futures::{prelude::stream::Select, Stream};
use pin_project_lite::pin_project;
use tokio_stream::wrappers::BroadcastStream;
use xmtp_common::{retry_async, Retry};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::{
    api_client::{trait_impls::XmtpApi, XmtpMlsStreams},
    xmtp::mls::api::v1::{welcome_message::V1 as WelcomeMessageV1, WelcomeMessage},
};

use super::{temp::Result, FutureWrapper, LocalEvents, SubscribeError};

#[derive(Debug)]
pub(super) enum WelcomeOrGroup {
    Group(Vec<u8>),
    Welcome(Result<WelcomeMessage>),
}

pin_project! {
    /// Broadcast stream filtered + mapped to WelcomeOrGroup
    pub(super) struct BroadcastGroupStream {
        #[pin] inner: BroadcastStream<LocalEvents>,
    }
}

impl BroadcastGroupStream {
    fn new(inner: BroadcastStream<LocalEvents>) -> Self {
        Self { inner }
    }
}

impl Stream for BroadcastGroupStream {
    type Item = WelcomeOrGroup;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use std::task::Poll::*;
        let this = self.project();

        match this.inner.poll_next(cx) {
            Ready(Some(event)) => {
                let ev = xmtp_common::optify!(event, "Missed messages due to event queue lag")
                    .and_then(LocalEvents::group_filter);
                if let Some(g) = ev {
                    Ready(Some(WelcomeOrGroup::Group(g)))
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
    pub(super) struct SubscriptionStream<S> {
        #[pin] inner: S,
    }
}

impl<S> SubscriptionStream<S> {
    fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S> Stream for SubscriptionStream<S>
where
    S: Stream<Item = std::result::Result<WelcomeMessage, xmtp_proto::Error>>,
{
    type Item = WelcomeOrGroup;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
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
        client: C,
        #[pin] inner: Subscription,
        conversation_type: Option<ConversationType>,
        known_welcome_ids: HashSet<i64>,
        #[pin] state: ProcessState<'a, C>,
    }
}

pin_project! {
    #[project = ProcessProject]
    enum ProcessState<'a, C> {
        /// State that indicates the stream is waiting on the next message from the network
        Waiting,
        /// State that indicates the stream is waiting on a IO/Network future to finish processing the current message
        /// before moving on to the next one
        Processing {
            #[pin] future: FutureWrapper<'a, Result<(MlsGroup<C>, Option<i64>)>>
        }
    }
}

// we can't avoid the cfg(target_arch) without making the entire
// 'process_new_item' flow a Future, which makes this code
// significantly more difficult to modify. The other option is storing a
// anonymous stack type in a struct that would be returned from an async fn
// struct Foo {
//      inner: impl Future
// }
// or some equivalent, which does not exist in rust.
//
// Another option is to make processing a welcome syncronous which
// might be possible with some kind of a cached identity strategy

impl<'a, O> Default for ProcessState<'a, O> {
    fn default() -> Self {
        ProcessState::Waiting
    }
}

type MultiplexedSelect<S> = Select<BroadcastGroupStream, SubscriptionStream<S>>;

pub(super) type WelcomesApiSubscription<'a, A> =
    MultiplexedSelect<<A as XmtpMlsStreams>::WelcomeMessageStream<'a>>;

impl<'a, A, V> StreamConversations<'a, Client<A, V>, WelcomesApiSubscription<'a, A>>
where
    A: XmtpApi + XmtpMlsStreams + Send + Sync + 'static,
    V: SmartContractSignatureVerifier + Send + Sync + 'static,
{
    pub async fn new(
        client: &'a Client<A, V>,
        conversation_type: Option<ConversationType>,
    ) -> Result<Self> {
        let provider = client.mls_provider()?;
        let conn = provider.conn_ref();
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
            client: client.clone(),
            inner: stream,
            known_welcome_ids,
            conversation_type,
            state: ProcessState::Waiting,
        })
    }
}

impl<'a, C, Subscription> Stream for StreamConversations<'a, C, Subscription>
where
    C: ScopedGroupClient + Clone + 'a,
    Subscription: Stream<Item = WelcomeOrGroup> + 'a,
{
    type Item = Result<MlsGroup<C>>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll::*;
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
                            // TODO: try ref here?
                            Self::process_new_item(this.known_welcome_ids.clone(), this.client.clone(), item);

                        this.state.set(ProcessState::Processing {
                            future: FutureWrapper::new(future),
                        });
                        cx.waker().wake_by_ref();
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

/// bulk of the processing for a new welcome/group
impl<'a, C, Subscription> StreamConversations<'a, C, Subscription>
where
    C: ScopedGroupClient + Clone,
{
    async fn process_new_item(
        known_welcome_ids: HashSet<i64>,
        client: C,
        item: WelcomeOrGroup,
    ) -> Result<(MlsGroup<C>, Option<i64>)> {
        use WelcomeOrGroup::*;
        let provider = client.context().mls_provider()?;
        match item {
            Welcome(w) => Self::on_welcome(&known_welcome_ids, client, &provider, w?)
                .await
                .map(|(g, w_id)| (g, Some(w_id))),
            Group(id) => {
                let (group, stored_group) = MlsGroup::new_validated(client, id, &provider)?;
                Ok((group, stored_group.welcome_id))
            }
        }
    }

    /// process a new welcome, returning the Group & Welcome ID
    pub(super) async fn on_welcome(
        known_welcome_ids: &HashSet<i64>,
        client: C,
        provider: &XmtpOpenMlsProvider,
        welcome: WelcomeMessage,
    ) -> Result<(MlsGroup<C>, i64)> {
        let WelcomeMessageV1 {
            id,
            created_ns: _,
            ref installation_key,
            ref data,
            ref hpke_public_key,
        } = super::extract_welcome_message(welcome)?;
        let id = id as i64;

        // TODO: Test multiple streams at once
        if known_welcome_ids.contains(&id) {
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
                MlsGroup::new(client.clone(), group.id, group.created_at_ns),
                id,
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

                client
                    .context()
                    .store()
                    .transaction_async(provider, |provider| async move {
                        MlsGroup::create_from_encrypted_welcome(
                            c,
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

        Ok((mls_group, id))
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use super::*;
    use crate::builder::ClientBuilder;
    use crate::groups::GroupMetadataOptions;
    use futures::StreamExt;
    use wasm_bindgen_test::wasm_bindgen_test;
    use xmtp_cryptography::utils::generate_local_wallet;

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread", worker_threads = 10))]
    async fn test_stream_welcomes() {
        let alice = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bob = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let alice_bob_group = alice
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        let mut stream = StreamConversations::new(&bob, None).await.unwrap();
        // futures::pin_mut!(stream);
        let group_id = alice_bob_group.group_id.clone();
        alice_bob_group
            .add_members_by_inbox_id(&[bob.inbox_id()])
            .await
            .unwrap();
        let bob_received_groups = stream.next().await.unwrap().unwrap();
        assert_eq!(bob_received_groups.group_id, group_id);
    }
}
