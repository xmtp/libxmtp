use super::{LocalEvents, Result, SubscribeError};
use crate::{
    groups::{scoped_client::ScopedGroupClient, MlsGroup},
    Client,
};
use xmtp_db::{group::ConversationType, refresh_state::EntityKind, NotFound, XmtpDb};

use futures::{prelude::stream::Select, Stream};
use pin_project_lite::pin_project;
use std::{
    collections::HashSet,
    future::Future,
    pin::Pin,
    task::{ready, Context, Poll},
};
use tokio_stream::wrappers::BroadcastStream;
use xmtp_common::{retry_async, FutureWrapper, Retry};
use xmtp_proto::{
    api_client::{trait_impls::XmtpApi, XmtpMlsStreams},
    xmtp::mls::api::v1::{welcome_message, WelcomeMessage},
};

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
        let mut this = self.project();
        // loop until the inner stream returns:
        // - Ready with a group
        // - Ready(None) - stream ended
        // ignore None values, since it is not a group, but may indicate more values in the stream
        // itself
        loop {
            if let Some(event) = ready!(this.inner.as_mut().poll_next(cx)) {
                if let Some(group) =
                    xmtp_common::optify!(event, "Missed messages due to event queue lag")
                        .and_then(LocalEvents::group_filter)
                {
                    return Ready(Some(Ok(WelcomeOrGroup::Group(group))));
                }
            } else {
                return Ready(None);
            }
        }
    }
}

pin_project! {
    /// Subscription Stream mapped to WelcomeOrGroup
    pub(super) struct SubscriptionStream<S, E> {
        #[pin] inner: S,
        _marker: std::marker::PhantomData<E>
    }
}

impl<S, E> SubscriptionStream<S, E> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<S, E> Stream for SubscriptionStream<S, E>
where
    S: Stream<Item = std::result::Result<WelcomeMessage, E>>,
    E: xmtp_common::RetryableError + Send + Sync + 'static,
{
    type Item = Result<WelcomeOrGroup>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use std::task::Poll::*;
        let this = self.project();

        match this.inner.poll_next(cx) {
            Ready(Some(welcome)) => {
                let welcome = welcome.map_err(|e| SubscribeError::BoxError(Box::new(e)))?;
                Ready(Some(Ok(WelcomeOrGroup::Welcome(welcome))))
            }
            Pending => Pending,
            Ready(None) => Ready(None),
        }
    }
}

pin_project! {
    pub struct StreamConversations<'a, C, Subscription> {
        #[pin] inner: Subscription,
        #[pin] state: ProcessState<'a, C>,
        client: C,
        conversation_type: Option<ConversationType>,
        known_welcome_ids: HashSet<i64>,
    }
}

pin_project! {
    #[project = ProcessProject]
    #[derive(Default)]
    enum ProcessState<'a, C> {
        /// State that indicates the stream is waiting on the next message from the network
        #[default]
        Waiting,
        /// State that indicates the stream is waiting on a IO/Network future to finish processing the current message
        /// before moving on to the next one
        Processing {
            #[pin] future: FutureWrapper<'a, Result<ProcessWelcomeResult<C>>>
        }
    }
}

type MultiplexedSelect<S, E> = Select<BroadcastGroupStream, SubscriptionStream<S, E>>;

pub(super) type WelcomesApiSubscription<'a, C> = MultiplexedSelect<
    <<C as ScopedGroupClient>::ApiClient as XmtpMlsStreams>::WelcomeMessageStream<'a>,
    <<C as ScopedGroupClient>::ApiClient as XmtpMlsStreams>::Error,
>;

impl<'a, A, D> StreamConversations<'a, Client<A, D>, WelcomesApiSubscription<'a, Client<A, D>>>
where
    A: XmtpApi + XmtpMlsStreams + Send + Sync + 'static,
    D: XmtpDb + Send + 'static,
{
    pub async fn new(
        client: &'a Client<A, D>,
        conversation_type: Option<ConversationType>,
    ) -> Result<Self> {
        let provider = client.mls_provider();
        let conn = provider.db();
        let installation_key = client.installation_public_key();
        let id_cursor = provider
            .db()
            .get_last_cursor_for_id(installation_key, EntityKind::Welcome)?;
        tracing::debug!(
            cursor = id_cursor,
            inbox_id = client.inbox_id(),
            "Setting up conversation stream cursor = {}",
            id_cursor
        );

        let events =
            BroadcastGroupStream::new(BroadcastStream::new(client.local_events.subscribe()));

        let subscription = client
            .api_client
            .subscribe_welcome_messages(installation_key.as_ref(), Some(id_cursor as u64))
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
        use ProcessProject::*;

        let this = self.as_mut().project();
        let state = this.state.project();

        match state {
            Waiting => {
                match this.inner.poll_next(cx) {
                    Ready(Some(item)) => {
                        let mut this = self.as_mut().project();
                        let future = ProcessWelcomeFuture::new(
                            this.known_welcome_ids.clone(),
                            this.client.clone(),
                            item?,
                            *this.conversation_type,
                        )?;

                        this.state.set(ProcessState::Processing {
                            future: FutureWrapper::new(future.process()),
                        });
                        // try to process the future immediately
                        // this will return immediately if we have already processed the welcome
                        // and it exists in the db
                        let Processing { future } = this.state.project() else {
                            unreachable!("Streaming processing future should exist.")
                        };
                        let poll = future.poll(cx);
                        self.as_mut().try_process(poll, cx)
                    }
                    // stream ended
                    Ready(None) => Ready(None),
                    Pending => Pending,
                }
            }
            Processing { future } => {
                let poll = future.poll(cx);
                self.as_mut().try_process(poll, cx)
            }
        }
    }
}

pub enum ProcessWelcomeResult<C> {
    /// New Group and welcome id
    New { group: MlsGroup<C>, id: i64 },
    /// A group we already have/we created that might not have a welcome id
    NewStored {
        group: MlsGroup<C>,
        maybe_id: Option<i64>,
    },
    /// Skip this welcome but add and id to known welcome ids
    IgnoreId { id: i64 },
    /// Skip this payload
    Ignore,
}

impl<'a, C, Subscription> StreamConversations<'a, C, Subscription>
where
    C: ScopedGroupClient + Clone + 'a,
    Subscription: Stream<Item = Result<WelcomeOrGroup>> + 'a,
{
    /// Try to process the welcome future
    #[allow(clippy::type_complexity)]
    fn try_process(
        mut self: Pin<&mut Self>,
        poll: Poll<Result<ProcessWelcomeResult<C>>>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<<Self as Stream>::Item>> {
        use Poll::*;
        let mut this = self.as_mut().project();
        match poll {
            Ready(Ok(ProcessWelcomeResult::New {
                group,
                id: welcome_id,
            })) => {
                tracing::debug!(
                    group_id = hex::encode(&group.group_id),
                    "finished processing with group {}",
                    hex::encode(&group.group_id)
                );
                this.known_welcome_ids.insert(welcome_id);
                this.state.set(ProcessState::Waiting);
                Ready(Some(Ok(group)))
            }
            // we are ignoring this payload with id
            Ready(Ok(ProcessWelcomeResult::IgnoreId { id })) => {
                tracing::debug!("ignoring streamed conversation payload with welcome id {id}");
                this.known_welcome_ids.insert(id);
                this.state.as_mut().set(ProcessState::Waiting);
                // we have to re-ad this task to the queue
                // to let http know we are waiting on the next item
                self.poll_next(cx)
            }
            Ready(Ok(ProcessWelcomeResult::Ignore)) => {
                tracing::debug!("ignoring streamed conversation payload");
                this.state.as_mut().set(ProcessState::Waiting);
                // we have to re-ad this task to the queue
                // to let http know we are waiting on the next item
                self.poll_next(cx)
            }
            Ready(Ok(ProcessWelcomeResult::NewStored { group, maybe_id })) => {
                tracing::debug!(
                    group_id = hex::encode(&group.group_id),
                    "finished processing with group {}",
                    hex::encode(&group.group_id)
                );
                if let Some(id) = maybe_id {
                    this.known_welcome_ids.insert(id);
                }
                this.state.set(ProcessState::Waiting);
                Ready(Some(Ok(group)))
            }
            Ready(Err(e)) => {
                this.state.as_mut().set(ProcessState::Waiting);
                Ready(Some(Err(e)))
            }
            Pending => Pending,
        }
    }
}

fn extract_welcome_message(welcome: &WelcomeMessage) -> Result<&welcome_message::V1> {
    match welcome.version {
        Some(welcome_message::Version::V1(ref welcome)) => Ok(welcome),
        _ => Err(ConversationStreamError::InvalidPayload.into()),
    }
}

/// Future for processing `WelcomeorGroup`
pub struct ProcessWelcomeFuture<Client> {
    /// welcome ids in DB and which are already processed
    known_welcome_ids: HashSet<i64>,
    /// The libxmtp client
    client: Client,
    /// the welcome or group being processed in this future
    item: WelcomeOrGroup,
    /// Conversation type to filter for, if any.
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
        Ok(Self {
            known_welcome_ids,
            client,
            item,
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
    #[tracing::instrument(skip_all)]
    pub async fn process(self) -> Result<ProcessWelcomeResult<C>> {
        use WelcomeOrGroup::*;
        let process_result = match self.item {
            Welcome(ref w) => {
                let welcome = extract_welcome_message(w)?;
                let id = welcome.id as i64;
                tracing::debug!("got welcome with id {}", id);
                // try to load it from store first and avoid overhead
                // of processing a welcome & erroring
                // for immediate return, this must stay in the top-level future,
                // to avoid a possible yield on the await in on_welcome.
                if self.known_welcome_ids.contains(&id) {
                    tracing::debug!(
                        "Found existing welcome. Returning from db & skipping processing"
                    );
                    let (group, id) = self.load_from_store(id)?;
                    return self.filter(ProcessWelcomeResult::New { group, id }).await;
                }
                // sync welcome from the network
                let (group, id) = self.on_welcome(welcome).await?;
                ProcessWelcomeResult::New { group, id }
            }
            Group(ref id) => {
                tracing::debug!("Stream conversations got existing group, pulling from db.");
                let (group, stored_group) =
                    MlsGroup::new_validated(self.client.clone(), id.to_vec())?;

                ProcessWelcomeResult::NewStored {
                    group,
                    maybe_id: stored_group.welcome_id,
                }
            }
        };
        self.filter(process_result).await
    }

    /// Filter for streamed conversations
    async fn filter(&self, processed: ProcessWelcomeResult<C>) -> Result<ProcessWelcomeResult<C>> {
        use ProcessWelcomeResult::*;
        match processed {
            New { group, id } => {
                let metadata = group.metadata().await?;
                // If it's a duplicate DM, don’t stream
                if metadata.conversation_type == ConversationType::Dm
                    && self.client.db().has_duplicate_dm(&group.group_id)?
                {
                    tracing::debug!("Duplicate DM group detected from Group(id). Skipping stream.");
                    return Ok(ProcessWelcomeResult::IgnoreId { id });
                }

                if self
                    .conversation_type
                    .is_none_or(|ct| ct == metadata.conversation_type)
                {
                    Ok(ProcessWelcomeResult::New { group, id })
                } else {
                    Ok(ProcessWelcomeResult::IgnoreId { id })
                }
            }
            NewStored { group, maybe_id } => {
                let metadata = group.metadata().await?;
                // If it's a duplicate DM, don’t stream
                if metadata.conversation_type == ConversationType::Dm
                    && self.client.db().has_duplicate_dm(&group.group_id)?
                {
                    tracing::debug!("Duplicate DM group detected from Group(id). Skipping stream.");
                    if let Some(id) = maybe_id {
                        return Ok(ProcessWelcomeResult::IgnoreId { id });
                    } else {
                        return Ok(ProcessWelcomeResult::Ignore);
                    }
                }

                if self
                    .conversation_type
                    .is_none_or(|ct| ct == metadata.conversation_type)
                {
                    Ok(ProcessWelcomeResult::NewStored { group, maybe_id })
                } else if let Some(id) = maybe_id {
                    Ok(ProcessWelcomeResult::IgnoreId { id })
                } else {
                    Ok(ProcessWelcomeResult::Ignore)
                }
            }
            other => Ok(other),
        }
    }

    /// process a new welcome, returning the Group & Welcome ID
    async fn on_welcome(&self, welcome: &welcome_message::V1) -> Result<(MlsGroup<C>, i64)> {
        let welcome_message::V1 {
            id,
            created_ns: _,
            ref installation_key,
            ..
        } = welcome;
        let id = *id as i64;

        let Self { ref client, .. } = self;
        tracing::info!(
            installation_id = hex::encode(installation_key),
            welcome_id = &id,
            "Trying to process streamed welcome"
        );

        retry_async!(Retry::default(), (async { client.sync_welcomes().await }))?;

        self.load_from_store(id)
    }

    /// Load a group from disk by its welcome_id
    fn load_from_store(&self, id: i64) -> Result<(MlsGroup<C>, i64)> {
        let provider = self.client.mls_provider();
        let group = provider
            .db()
            .find_group_by_welcome_id(id)?
            .ok_or(NotFound::GroupByWelcome(id))?;
        tracing::info!(
            inbox_id = self.client.inbox_id(),
            group_id = hex::encode(&group.id),
            dm_id = group.dm_id,
            welcome_id = ?group.welcome_id,
            "loading existing group for welcome_id: {:?}",
            group.welcome_id
        );
        Ok((
            MlsGroup::new(
                self.client.clone(),
                group.id,
                group.dm_id,
                group.created_at_ns,
            ),
            id,
        ))
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use super::*;
    use crate::builder::ClientBuilder;
    use crate::groups::GroupMetadataOptions;
    use crate::tester;
    use xmtp_db::group::GroupQueryArgs;

    use futures::StreamExt;
    use xmtp_cryptography::utils::generate_local_wallet;

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    #[rstest::rstest]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(5))]
    async fn test_stream_welcomes() {
        let alice = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bob = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let alice_bob_group = alice
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        let mut stream = StreamConversations::new(&bob, None).await.unwrap();
        let group_id = alice_bob_group.group_id.clone();
        alice_bob_group
            .add_members_by_inbox_id(&[bob.inbox_id()])
            .await
            .unwrap();
        tracing::info!("WAITING FOR NEXT STREAM ITEM");
        let bob_received_groups = stream.next().await.unwrap().unwrap();
        assert_eq!(bob_received_groups.group_id, group_id);
    }

    #[rstest::rstest]
    #[case(ConversationType::Dm, "Unexpectedly received a Group")]
    #[case(ConversationType::Group, "Unexpectedly received a DM")]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(7))]

    // #[cfg_attr(target_arch = "wasm32", ignore)]
    async fn test_dm_stream_filter(
        #[case] conversation_type: ConversationType,
        #[case] expected: &str,
    ) {
        tester!(alix);
        tester!(bo);
        let stream = alix
            .stream_conversations(Some(conversation_type))
            .await
            .unwrap();
        futures::pin_mut!(stream);

        alix.find_or_create_dm_by_inbox_id(bo.inbox_id().to_string(), None)
            .await
            .unwrap();

        let group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();

        let group = stream.next().await.unwrap();
        let metadata = group.unwrap().metadata().await.unwrap();

        assert_eq!(
            metadata.conversation_type, conversation_type,
            "{}",
            expected
        );
        // there is only one item on the stream
        let result =
            xmtp_common::time::timeout(std::time::Duration::from_millis(100), stream.next()).await;
        assert!(result.is_err(), "should only be one item in the stream");
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(7))]
    #[cfg_attr(target_arch = "wasm32", ignore)]
    async fn test_dm_stream_all_conversation_types() {
        let alix = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bo = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let davon = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let eri = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        // Start a stream with all conversations
        let mut groups = Vec::new();
        // Wait for 2 seconds for the group creation to be streamed
        let stream = alix.stream_conversations(None).await.unwrap();
        futures::pin_mut!(stream);

        alix.find_or_create_dm_by_inbox_id(davon.inbox_id().to_string(), None)
            .await
            .unwrap();
        let group = stream.next().await.unwrap();
        assert!(group.is_ok());
        groups.push(group.unwrap());

        let dm = eri
            .find_or_create_dm_by_inbox_id(alix.inbox_id().to_string(), None)
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

    #[rstest::rstest]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(10))]
    async fn test_self_group_creation() {
        let alix = Arc::new(ClientBuilder::new_test_client_no_sync(&generate_local_wallet()).await);
        let bo = Arc::new(ClientBuilder::new_test_client_no_sync(&generate_local_wallet()).await);

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
        alix.sync_welcomes().await.unwrap();
        let find_groups_results = alix.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(2, find_groups_results.len());
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(5))]
    async fn test_add_remove_re_add() {
        let alix = Arc::new(ClientBuilder::new_test_client_no_sync(&generate_local_wallet()).await);
        let bo = Arc::new(ClientBuilder::new_test_client_no_sync(&generate_local_wallet()).await);

        let alix_group = alix
            .create_group_with_inbox_ids(
                &[bo.inbox_id().to_string()],
                None,
                GroupMetadataOptions::default(),
            )
            .await
            .unwrap();

        alix_group
            .remove_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();
        bo.sync_welcomes().await.unwrap();
        let stream = bo
            .stream_conversations(Some(ConversationType::Group))
            .await
            .unwrap();
        futures::pin_mut!(stream);
        alix_group
            .add_members_by_inbox_id(&[bo.inbox_id().to_string()])
            .await
            .unwrap();

        let group = stream.next().await.unwrap();
        assert!(group.is_ok());
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(15))]
    async fn test_duplicate_dm_not_streamed() {
        use xmtp_cryptography::utils::generate_local_wallet;

        let client1 = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let client2 = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let mut stream = client1.stream_conversations(None).await.unwrap();

        // First DM - should stream
        let dm1 = client1
            .find_or_create_dm_by_inbox_id(client2.inbox_id().to_string(), None)
            .await
            .unwrap();

        let streamed_dm1 = stream.next().await.unwrap();
        assert!(streamed_dm1.is_ok());
        assert_eq!(streamed_dm1.unwrap().group_id, dm1.group_id);

        // Create a second DM with same participants — triggers duplicate logic
        let dm2 = client2
            .find_or_create_dm_by_inbox_id(client1.inbox_id().to_string(), None)
            .await
            .unwrap();

        // Make sure it's actually a new group
        assert_ne!(dm1.group_id, dm2.group_id);

        // It should NOT appear in the stream
        let result =
            xmtp_common::time::timeout(std::time::Duration::from_millis(100), stream.next()).await;
        assert!(result.is_err(), "Duplicate DM was unexpectedly streamed");
    }
}
