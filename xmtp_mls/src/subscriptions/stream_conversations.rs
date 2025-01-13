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
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::{
    api_client::{trait_impls::XmtpApi, XmtpMlsStreams},
    xmtp::mls::api::v1::{welcome_message, WelcomeMessage},
};

use super::{temp::Result, FutureWrapper, LocalEvents, SubscribeError};

#[derive(thiserror::Error, Debug)]
pub enum ConversationStreamError {
    #[error("unexpected message type in welcome")]
    InvalidPayload,
    #[error("the conversation was filtered because of the given conversation type")]
    InvalidConversationType,
}

impl xmtp_common::RetryableError for ConversationStreamError {
    fn is_retryable(&self) -> bool {
        use ConversationStreamError::*;
        match self {
            InvalidPayload | InvalidConversationType => false,
        }
    }
}

#[derive(Debug)]
pub(super) enum WelcomeOrGroup {
    Group(Vec<u8>),
    Welcome(WelcomeMessage),
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
    type Item = Result<WelcomeOrGroup>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use std::task::Poll::*;
        let this = self.project();

        match this.inner.poll_next(cx) {
            Ready(Some(event)) => {
                let ev = xmtp_common::optify!(event, "Missed messages due to event queue lag")
                    .and_then(LocalEvents::group_filter);
                if let Some(g) = ev {
                    Ready(Some(Ok(WelcomeOrGroup::Group(g))))
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
    type Item = Result<WelcomeOrGroup>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use std::task::Poll::*;
        let this = self.project();

        match this.inner.poll_next(cx) {
            Ready(Some(welcome)) => {
                let welcome = welcome.map_err(SubscribeError::from)?;
                Ready(Some(Ok(WelcomeOrGroup::Welcome(welcome))))
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
            #[pin] future: FutureWrapper<'a, Result<Option<(MlsGroup<C>, Option<i64>)>>>
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
    Subscription: Stream<Item = Result<WelcomeOrGroup>> + 'a,
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
                        let future = ProcessWelcomeFuture::new(
                            this.known_welcome_ids.clone(),
                            this.client.clone(),
                            item?,
                            *this.conversation_type,
                        )?;

                        this.state.set(ProcessState::Processing {
                            future: FutureWrapper::new(future.process()),
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
                Ready(Ok(Some((group, welcome_id)))) => {
                    if let Some(id) = welcome_id {
                        this.known_welcome_ids.insert(id);
                    }
                    this.state.set(ProcessState::Waiting);
                    Ready(Some(Ok(group)))
                }
                // we are ignoring this payload
                Ready(Ok(None)) => {
                    this.state.set(ProcessState::Waiting);
                    cx.waker().wake_by_ref();
                    Pending
                }
                Ready(Err(e)) => Ready(Some(Err(e))),
                Pending => Pending,
            },
        }
    }
}

fn extract_welcome_message<'a>(welcome: &'a WelcomeMessage) -> Result<&'a welcome_message::V1> {
    match welcome.version {
        Some(welcome_message::Version::V1(ref welcome)) => Ok(welcome),
        _ => Err(ConversationStreamError::InvalidPayload.into()),
    }
}

/// Future for processing `WelcomeorGroup`
pub struct ProcessWelcomeFuture<Client> {
    known_welcome_ids: HashSet<i64>,
    client: Client,
    item: WelcomeOrGroup,
    provider: XmtpOpenMlsProvider,
    conversation_type: Option<ConversationType>,
}

impl<C> ProcessWelcomeFuture<C>
where
    C: ScopedGroupClient + Clone,
{
    pub fn new(
        known_welcome_ids: HashSet<i64>,
        client: C,
        item: WelcomeOrGroup,
        conversation_type: Option<ConversationType>,
    ) -> Result<ProcessWelcomeFuture<C>> {
        let provider = client.context().mls_provider()?;

        Ok(Self {
            known_welcome_ids,
            client,
            item,
            provider,
            conversation_type,
        })
    }
}

/// bulk of the processing for a new welcome/group
impl<C> ProcessWelcomeFuture<C>
where
    C: ScopedGroupClient + Clone,
{
    /// Process the welcome. if its a group, create the group and return it.
    pub async fn process(self) -> Result<Option<(MlsGroup<C>, Option<i64>)>> {
        use WelcomeOrGroup::*;
        let (group, welcome_id) = match self.item {
            Welcome(ref w) => {
                let (group, id) = self.on_welcome(w).await?;
                (group, Some(id))
            }
            Group(id) => {
                let (group, stored_group) =
                    MlsGroup::new_validated(self.client, id, &self.provider)?;
                (group, stored_group.welcome_id)
            }
        };

        let metadata = group.metadata(&self.provider).await?;
        Ok(self
            .conversation_type
            .map_or(true, |ct| ct == metadata.conversation_type)
            .then_some((group, welcome_id)))
    }

    /// process a new welcome, returning the Group & Welcome ID
    async fn on_welcome(&self, welcome: &WelcomeMessage) -> Result<(MlsGroup<C>, i64)> {
        let welcome_message::V1 {
            id,
            created_ns: _,
            ref installation_key,
            ref data,
            ref hpke_public_key,
        } = extract_welcome_message(welcome)?;
        let id = *id as i64;

        // try to load it from store first and avoid overhead
        // of processing a welcome & erroring
        if self.known_welcome_ids.contains(&id) {
            return self.load_from_store(id);
        }

        let Self {
            ref client,
            ref provider,
            ..
        } = self;
        tracing::info!(
            installation_id = hex::encode(installation_key),
            welcome_id = &id,
            "Trying to process streamed welcome"
        );

        let group = client
            .context()
            .store()
            .retryable_transaction_async(provider, |provider| async {
                MlsGroup::create_from_encrypted_welcome(
                    client,
                    provider,
                    hpke_public_key.as_slice(),
                    data,
                    id,
                )
                .await
            })
            .await;

        if let Err(e) = group {
            // try to load it from the store again
            return self.load_from_store(id).map_err(|_| SubscribeError::from(e));
        }

        Ok((group?, id))
    }

    /// Load a group from disk by its welcome_id
    fn load_from_store(&self, id: i64) -> Result<(MlsGroup<C>, i64)> {
        let conn = self.provider.conn_ref();
        let group = conn
            .find_group_by_welcome_id(id)?
            .ok_or(NotFound::GroupByWelcome(id))?;
        tracing::info!(
            inbox_id = self.client.inbox_id(),
            group_id = hex::encode(&group.id),
            welcome_id = ?group.welcome_id,
            "Loading existing group for welcome_id: {:?}",
            group.welcome_id
        );
        return Ok((
            MlsGroup::new(self.client.clone(), group.id, group.created_at_ns),
            id,
        ));
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use super::*;
    use crate::builder::ClientBuilder;
    use crate::groups::GroupMetadataOptions;
    use crate::subscriptions::GroupQueryArgs;

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

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread"))]
    #[cfg_attr(target_family = "wasm", ignore)]
    async fn test_dm_streaming() {
        let alix = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bo = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let stream = alix
            .stream_conversations(Some(ConversationType::Group))
            .await
            .unwrap();
        futures::pin_mut!(stream);

        alix.create_dm_by_inbox_id(bo.inbox_id().to_string())
            .await
            .unwrap();
        let result =
            xmtp_common::time::timeout(std::time::Duration::from_millis(100), stream.next()).await;
        assert!(result.is_err(), "Stream unexpectedly received a DM group");

        let group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();

        let group = stream.next().await.unwrap();
        assert!(group.is_ok());

        // Start a stream with only dms
        // Start a stream with conversation_type DM
        let stream = alix
            .stream_conversations(Some(ConversationType::Dm))
            .await
            .unwrap();
        futures::pin_mut!(stream);

        let group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();

        // we should not get a message
        let result =
            xmtp_common::time::timeout(std::time::Duration::from_millis(100), stream.next()).await;
        assert!(result.is_err(), "Stream unexpectedly received a Group");

        alix.create_dm_by_inbox_id(bo.inbox_id().to_string())
            .await
            .unwrap();
        let group = stream.next().await.unwrap();
        assert!(group.is_ok());

        // Start a stream with all conversations
        let mut groups = Vec::new();
        // Wait for 2 seconds for the group creation to be streamed
        let stream = alix.stream_conversations(None).await.unwrap();
        futures::pin_mut!(stream);

        alix.create_dm_by_inbox_id(bo.inbox_id().to_string())
            .await
            .unwrap();
        let group = stream.next().await.unwrap();
        assert!(group.is_ok());
        groups.push(group.unwrap());

        let dm = bo
            .create_dm_by_inbox_id(alix.inbox_id().to_string())
            .await
            .unwrap();
        dm.add_members_by_inbox_id(&[alix.inbox_id()])
            .await
            .unwrap();
        let group = stream.next().await.unwrap();
        assert!(group.is_ok());
        groups.push(group.unwrap());

        let group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();
        let group = stream.next().await.unwrap();
        assert!(group.is_ok());
        groups.push(group.unwrap());

        assert_eq!(groups.len(), 3);
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread"))]
    async fn test_self_group_creation() {
        let alix = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bo = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let stream = alix
            .stream_conversations(Some(ConversationType::Group))
            .await
            .unwrap();
        futures::pin_mut!(stream);

        alix.create_group(None, GroupMetadataOptions::default())
            .unwrap();
        let _self_group = stream.next().await.unwrap();

        let group = bo
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&[alix.inbox_id()])
            .await
            .unwrap();
        let _bo_group = stream.next().await.unwrap();

        // Verify syncing welcomes while streaming causes no issues
        alix.sync_welcomes(&alix.mls_provider().unwrap())
            .await
            .unwrap();
        let find_groups_results = alix.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(2, find_groups_results.len());
    }
}
