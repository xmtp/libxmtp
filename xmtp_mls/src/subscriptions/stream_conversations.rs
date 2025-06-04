use super::{process_welcome::ProcessWelcomeResult, LocalEvents, Result, SubscribeError};
use crate::{
    context::XmtpMlsLocalContext, groups::MlsGroup,
    subscriptions::process_welcome::ProcessWelcomeFuture,
};
use xmtp_db::{group::ConversationType, refresh_state::EntityKind, XmtpDb};

use futures::{prelude::stream::Select, Stream};
use pin_project_lite::pin_project;
use std::{
    borrow::Cow,
    collections::HashSet,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
};
use tokio_stream::wrappers::BroadcastStream;
use xmtp_common::FutureWrapper;
use xmtp_proto::{
    api_client::{trait_impls::XmtpApi, XmtpMlsStreams},
    xmtp::mls::api::v1::WelcomeMessage,
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

pub enum WelcomeOrGroup {
    Group(Vec<u8>),
    Welcome(WelcomeMessage),
}

impl std::fmt::Debug for WelcomeOrGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Group(arg0) => f.debug_tuple("Group").field(&hex::encode(arg0)).finish(),
            Self::Welcome(arg0) => f.debug_tuple("Welcome").field(arg0).finish(),
        }
    }
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
    /// The stream for conversations.
    /// Handles the state machine that processes welcome messages and groups. It handles
    /// two main states:
    ///
    /// - `Waiting`: Ready to receive the next message from the inner stream
    /// - `Processing`: Currently processing a welcome/group through a future
    ///
    /// The implementation ensures efficient processing by immediately attempting
    /// to advance futures when possible, rather than waiting for the next poll cycle.
    ///
    /// # Arguments
    /// * `cx` - The task context for polling
    ///
    /// # Returns
    /// * `Poll<Option<Result<MlsGroup<ApiClient, Db>>>>` - The polling result:
    ///   - `Ready(Some(Ok(group)))` when a group is successfully processed
    ///   - `Ready(Some(Err(e)))` when an error occurs
    ///   - `Pending` when waiting for more data or for future completion
    ///   - `Ready(None)` when the stream has ended
    pub struct StreamConversations<'a, ApiClient, Db, Subscription> {
        #[pin] inner: Subscription,
        #[pin] state: ProcessState<'a, ApiClient, Db>,
        context: Cow<'a, Arc<XmtpMlsLocalContext<ApiClient, Db>>>,
        conversation_type: Option<ConversationType>,
        known_welcome_ids: HashSet<i64>,
    }
}

pin_project! {
    #[project = ProcessProject]
    #[derive(Default)]
    enum ProcessState<'a, ApiClient, Db> {
        /// State that indicates the stream is waiting on the next message from the network
        #[default]
        Waiting,
        /// State that indicates the stream is waiting on a IO/Network future to finish processing the current message
        /// before moving on to the next one
        Processing {
            #[pin] future: FutureWrapper<'a, Result<ProcessWelcomeResult<ApiClient, Db>>>
        }
    }
}

type MultiplexedSelect<S, E> = Select<BroadcastGroupStream, SubscriptionStream<S, E>>;

pub(super) type WelcomesApiSubscription<'a, ApiClient> = MultiplexedSelect<
    <ApiClient as XmtpMlsStreams>::WelcomeMessageStream,
    <ApiClient as XmtpMlsStreams>::Error,
>;

impl<'a, A, D> StreamConversations<'a, A, D, WelcomesApiSubscription<'a, A>>
where
    A: XmtpApi + XmtpMlsStreams + Send + Sync + 'a,
    D: XmtpDb + Send + 'a,
{
    /// Creates a new welcome message and conversation stream.
    ///
    /// This function initializes a stream that combines local and remote events
    /// for receiving conversation updates. It handles both welcome messages from
    /// the network and locally generated group events.
    ///
    /// Key initialization steps:
    /// 1. Retrieves the last cursor position for welcome messages
    /// 2. Sets up a broadcast stream for internal events
    /// 3. Creates a network subscription starting from the cursor
    /// 4. Loads existing welcome IDs to prevent reprocessing
    /// 5. Combines these sources into a multiplexed stream
    ///
    /// # Arguments
    /// * `client` - Reference to the client used for API communication
    /// * `conversation_type` - Optional filter to only receive specific conversation types
    ///
    /// # Returns
    /// * `Result<Self>` - A new conversation stream if successful
    ///
    /// # Errors
    /// May return errors if:
    /// - Database operations fail
    /// - API subscription creation fails
    ///
    /// # Example
    /// ```
    /// let stream = StreamConversations::new(&client, Some(ConversationType::Dm)).await?;
    /// ```
    pub async fn new(
        context: &'a Arc<XmtpMlsLocalContext<A, D>>,
        conversation_type: Option<ConversationType>,
    ) -> Result<Self> {
        Self::init(Cow::Borrowed(context), conversation_type).await
    }

    async fn init(
        context: Cow<'a, Arc<XmtpMlsLocalContext<A, D>>>,
        conversation_type: Option<ConversationType>,
    ) -> Result<Self> {
        let provider = context.mls_provider();
        let conn = provider.db();
        let installation_key = context.installation_public_key();
        let id_cursor = provider
            .db()
            .get_last_cursor_for_id(installation_key, EntityKind::Welcome)?;
        tracing::debug!(
            cursor = id_cursor,
            inbox_id = context.inbox_id(),
            "Setting up conversation stream cursor = {}",
            id_cursor
        );

        let events =
            BroadcastGroupStream::new(BroadcastStream::new(context.local_events.subscribe()));

        let subscription = context
            .as_ref()
            .api_client
            .subscribe_welcome_messages(installation_key.as_ref(), Some(id_cursor as u64))
            .await?;
        let subscription = SubscriptionStream::new(subscription);
        let known_welcome_ids = HashSet::from_iter(conn.group_welcome_ids()?.into_iter());

        let stream = futures::stream::select(events, subscription);

        Ok(Self {
            context,
            inner: stream,
            known_welcome_ids,
            conversation_type,
            state: ProcessState::Waiting,
        })
    }
}

impl<A, D> StreamConversations<'static, A, D, WelcomesApiSubscription<'static, A>>
where
    A: XmtpApi + XmtpMlsStreams + Send + Sync + 'static,
    D: XmtpDb + Send + 'static,
{
    pub async fn new_owned(
        context: Arc<XmtpMlsLocalContext<A, D>>,
        conversation_type: Option<ConversationType>,
    ) -> Result<Self> {
        Self::init(Cow::Owned(context), conversation_type).await
    }
}

impl<'a, ApiClient, Db, Subscription> Stream
    for StreamConversations<'a, ApiClient, Db, Subscription>
where
    ApiClient: XmtpApi,
    Db: XmtpDb,
    Subscription: Stream<Item = Result<WelcomeOrGroup>> + 'a,
{
    type Item = Result<MlsGroup<ApiClient, Db>>;

    #[tracing::instrument(skip_all, name = "poll_next_stream_conversations" level = "trace")]
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
                        tracing::info!("New Welcome: {:?}", item);
                        let mut this = self.as_mut().project();
                        let future = ProcessWelcomeFuture::new(
                            this.known_welcome_ids.clone(),
                            this.context.clone().into_owned(),
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

impl<'a, ApiClient, Db, Subscription> StreamConversations<'a, ApiClient, Db, Subscription>
where
    ApiClient: XmtpApi + 'a,
    Db: XmtpDb + 'a,
    Subscription: Stream<Item = Result<WelcomeOrGroup>> + 'a,
{
    /// Processes the result of a welcome future.
    ///
    /// This method handles the state transitions and output generation based on
    /// the result of processing a welcome message or group. It implements the core
    /// logic for determining what to do with each processed welcome:
    ///
    /// - For new groups: Updates tracking and yields the group
    /// - For groups to ignore: Updates tracking and continues polling
    /// - For previously stored groups: Yields the group
    /// - For errors: Propagates the error and returns to waiting state
    ///
    /// # Arguments
    /// * `poll` - The polling result from the welcome processing future
    /// * `cx` - The task context for polling
    ///
    /// # Returns
    /// * `Poll<Option<Result<MlsGroup<ApiClient, Db>>>>` - The polling result based on
    ///   the welcome processing outcome
    ///
    /// # Note
    /// This method is critical for maintaining the stream's state machine and
    /// ensuring proper handling of all possible processing outcomes.
    fn try_process(
        mut self: Pin<&mut Self>,
        poll: Poll<Result<ProcessWelcomeResult<ApiClient, Db>>>,
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
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Ready(Ok(ProcessWelcomeResult::Ignore)) => {
                tracing::debug!("ignoring streamed conversation payload");
                this.state.as_mut().set(ProcessState::Waiting);
                // we have to re-ad this task to the queue
                // to let http know we are waiting on the next item
                cx.waker().wake_by_ref();
                Poll::Pending
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

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use super::*;
    use crate::builder::ClientBuilder;
    use crate::tester;
    use crate::utils::fixtures::{alix, bo};
    use crate::utils::FullXmtpClient;
    use xmtp_db::group::GroupQueryArgs;

    use futures::StreamExt;
    use xmtp_cryptography::utils::generate_local_wallet;

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    #[rstest::rstest]
    #[case::two_conversations(2)]
    #[case::five_conversations(5)]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(10))]
    #[cfg_attr(target_arch = "wasm32", ignore)]
    #[awt]
    async fn stream_welcomes(
        #[future] alix: FullXmtpClient,
        #[future] bo: FullXmtpClient,
        #[case] group_size: usize,
    ) {
        let mut groups = vec![];
        let mut stream = StreamConversations::new(&bo.context, None).await.unwrap();
        for _ in 0..group_size {
            let alix_bo_group = alix.create_group(None, None).unwrap();
            groups.push(alix_bo_group.group_id.clone());
            alix_bo_group
                .add_members_by_inbox_id(&[bo.inbox_id()])
                .await
                .unwrap();
        }
        while !groups.is_empty() {
            let bo_received_groups = stream.next().await.unwrap().unwrap();
            let index = groups
                .iter()
                .position(|group_id| bo_received_groups.group_id == *group_id)
                .expect("group must be found");
            groups.remove(index);
        }

        assert!(groups.is_empty());
    }

    #[rstest::rstest]
    #[xmtp_common::test(unwrap_try = "true")]
    async fn test_sync_groups_are_not_streamed() {
        tester!(alix, sync_worker);
        let stream = alix.stream_conversations(None).await?;
        futures::pin_mut!(stream);

        tester!(_alix2, from: alix);

        let result =
            xmtp_common::time::timeout(std::time::Duration::from_millis(100), stream.next()).await;
        assert!(result.is_err(), "Sync group should not stream");
    }

    #[rstest::rstest]
    #[case(ConversationType::Dm, "Unexpectedly received a Group")]
    #[case(ConversationType::Group, "Unexpectedly received a DM")]
    #[xmtp_common::test]
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

        let group = alix.create_group(None, None).unwrap();
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

        let group = alix.create_group(None, None).unwrap();
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

        alix.create_group(None, None).unwrap();
        let _self_group = stream.next().await.unwrap();

        let group = bo.create_group(None, None).unwrap();
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
            .create_group_with_inbox_ids(&[bo.inbox_id().to_string()], None, None)
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
