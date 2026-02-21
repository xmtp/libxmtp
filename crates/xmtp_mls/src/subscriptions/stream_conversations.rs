use super::{LocalEvents, Result, SubscribeError, process_welcome::ProcessWelcomeResult};
use crate::subscriptions::StreamKind;
use crate::{
    context::XmtpSharedContext, groups::MlsGroup,
    subscriptions::process_welcome::ProcessWelcomeFuture,
};
use xmtp_api_grpc::streams::{MultiplexedStream, multiplexed};
use xmtp_common::task::JoinSet;
use xmtp_db::{consent_record::ConsentState, group::ConversationType};

use futures::Stream;
use pin_project::{pin_project, pinned_drop};
use std::{
    borrow::Cow,
    collections::HashSet,
    pin::Pin,
    task::{Poll, ready},
};
use tokio_stream::wrappers::BroadcastStream;
use xmtp_common::{BoxDynFuture, Event, MaybeSend};
use xmtp_db::prelude::*;
use xmtp_macro::log_event;
use xmtp_proto::api_client::XmtpMlsStreams;
use xmtp_proto::types::{Cursor, OriginatorId, SequenceId, WelcomeMessage};

#[derive(thiserror::Error, Debug)]
pub enum ConversationStreamError {
    #[error("unexpected message type in welcome")]
    InvalidPayload,
    #[error("the conversation was filtered because of the given conversation type")]
    InvalidConversationType,
    #[error("the welcome pointer was not found")]
    WelcomePointerNotFound,
}

impl xmtp_common::RetryableError for ConversationStreamError {
    fn is_retryable(&self) -> bool {
        use ConversationStreamError::*;
        match self {
            InvalidPayload | InvalidConversationType => false,
            WelcomePointerNotFound => true,
        }
    }
}

pub enum WelcomeOrGroup {
    Group(Vec<u8>),
    Welcome(xmtp_proto::types::WelcomeMessage),
}

impl std::fmt::Debug for WelcomeOrGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Group(arg0) => f.debug_tuple("Group").field(&hex::encode(arg0)).finish(),
            Self::Welcome(arg0) => f.debug_tuple("Welcome").field(arg0).finish(),
        }
    }
}

#[pin_project]
/// Broadcast stream filtered + mapped to WelcomeOrGroup
pub struct BroadcastGroupStream {
    #[pin]
    inner: BroadcastStream<LocalEvents>,
}

impl BroadcastGroupStream {
    fn new(inner: BroadcastStream<LocalEvents>) -> Self {
        Self { inner }
    }
}

impl Stream for BroadcastGroupStream {
    type Item = Result<WelcomeOrGroup>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
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

#[pin_project]
/// Subscription Stream mapped to WelcomeOrGroup
pub struct SubscriptionStream<S, E> {
    #[pin]
    inner: S,
    _marker: std::marker::PhantomData<E>,
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
    S: Stream<Item = std::result::Result<WelcomeMessage, E>> + MaybeSend,
    E: xmtp_common::RetryableError + 'static,
{
    type Item = Result<WelcomeOrGroup>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
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
/// * `Poll<Option<Result<MlsGroup<Context>>>>` - The polling result:
///   - `Ready(Some(Ok(group)))` when a group is successfully processed
///   - `Ready(Some(Err(e)))` when an error occurs
///   - `Pending` when waiting for more data or for future completion
///   - `Ready(None)` when the stream has ended
#[pin_project(PinnedDrop)]
pub struct StreamConversations<'a, Context: Clone + XmtpSharedContext, Subscription> {
    #[pin]
    inner: Subscription,
    context: Cow<'a, Context>,
    #[pin]
    welcome_syncs: JoinSet<Result<ProcessWelcomeResult<Context>>>,
    conversation_type: Option<ConversationType>,
    known_welcome_ids: HashSet<Cursor>,
    include_duplicated_dms: bool,
    consent_states: Option<Vec<ConsentState>>,
}

#[pinned_drop]
impl<'a, Context, Subscription> PinnedDrop for StreamConversations<'a, Context, Subscription>
where
    Context: Clone + XmtpSharedContext,
{
    fn drop(self: Pin<&mut Self>) {
        log_event!(
            Event::StreamClosed,
            self.context.installation_id(),
            kind = ?StreamKind::Conversations
        );
    }
}

#[pin_project(project = ProcessProject)]
#[derive(Default)]
enum ProcessState<'a, Context> {
    /// State that indicates the stream is waiting on the next message from the network
    #[default]
    Waiting,
    /// State that indicates the stream is waiting on a IO/Network future to finish processing the current message
    /// before moving on to the next one
    #[allow(unused)]
    Processing {
        #[pin]
        future: BoxDynFuture<'a, Result<ProcessWelcomeResult<Context>>>,
    },
}

pub(super) type WelcomesApiSubscription<'a, ApiClient> = MultiplexedStream<
    SubscriptionStream<
        <ApiClient as XmtpMlsStreams>::WelcomeMessageStream,
        <ApiClient as XmtpMlsStreams>::Error,
    >,
    BroadcastGroupStream,
>;

impl<'a, C> StreamConversations<'a, C, WelcomesApiSubscription<'a, C::ApiClient>>
where
    C: XmtpSharedContext + 'a,
    C::ApiClient: XmtpMlsStreams + 'a,
    C::Db: 'a,
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
    /// * `include_duplicate_dms` - Optional filter to include duplicate dms in the stream
    /// * `consent_states` - Optional filter to only receive conversations with specific consent states
    ///
    /// # Returns
    /// * `Result<Self>` - A new conversation stream if successful
    ///
    /// # Errors
    /// May return errors if:
    /// - Database operations fail
    /// - API subscription creation fails
    ///
    pub async fn new(
        context: &'a C,
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<Self> {
        log_event!(
            Event::StreamOpened,
            context.installation_id(),
            kind = ?StreamKind::Conversations
        );
        Self::from_cow(
            Cow::Borrowed(context),
            conversation_type,
            include_duplicate_dms,
            consent_states,
        )
        .await
    }

    pub async fn from_cow(
        context: Cow<'a, C>,
        conversation_type: Option<ConversationType>,
        include_duplicated_dms: bool,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<Self> {
        let conn = context.db();
        let installation_key = context.installation_id();
        tracing::debug!(
            inbox_id = context.inbox_id(),
            "Setting up conversation stream cursor",
        );

        let events =
            BroadcastGroupStream::new(BroadcastStream::new(context.local_events().subscribe()));

        let subscription = context
            .api()
            .subscribe_welcome_messages(&installation_key)
            .await?;
        let subscription = SubscriptionStream::new(subscription);
        let known_welcome_ids = HashSet::from_iter(conn.group_cursors()?.into_iter());

        let stream = multiplexed(subscription, events);

        Ok(Self {
            context,
            inner: stream,
            known_welcome_ids,
            conversation_type,
            welcome_syncs: JoinSet::new(),
            include_duplicated_dms,
            consent_states,
        })
    }
}

impl<C> StreamConversations<'static, C, WelcomesApiSubscription<'static, C::ApiClient>>
where
    C: XmtpSharedContext + 'static,
    C::ApiClient: XmtpMlsStreams + 'static,
    C::Db: 'static,
{
    pub async fn new_owned(
        context: C,
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<Self> {
        Self::from_cow(
            Cow::Owned(context),
            conversation_type,
            include_duplicate_dms,
            consent_states,
        )
        .await
    }
}

impl<'a, C, Subscription> Stream for StreamConversations<'a, C, Subscription>
where
    C: XmtpSharedContext + 'static,
    Subscription: Stream<Item = Result<WelcomeOrGroup>> + 'static,
    C::ApiClient: 'static,
    C::Db: 'static,
{
    type Item = Result<MlsGroup<C>>;

    #[tracing::instrument(skip_all, name = "poll_next_stream_conversations" level = "trace")]
    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        // We don't care if this is:
        // - Pending: we return pending by-default in the next section
        // - Ready(None): this just means the JoinSet is empty (no welcome syncs ongoing)
        // - Ready(Some(Err(welcome_result))): processing the welcome failed and the task failed with
        // a panic/error, we just ignore this.
        if let Poll::Ready(Some(Ok(welcome_result))) =
            self.as_mut().project().welcome_syncs.poll_join_next(cx)
        {
            // if filter is None, we continue to poll the inner stream.
            // the inner stream propagates a Pending, if its not pending, we register the task for
            // wakeup again. Therefore, we can ignore the None.
            if let Some(new_welcome) = self.as_mut().filter_welcome(welcome_result) {
                return Poll::Ready(Some(new_welcome));
            }
        }

        let mut this = self.as_mut().project();
        match ready!(this.inner.poll_next(cx)) {
            Some(welcome_envelope) => {
                let future = ProcessWelcomeFuture::new(
                    this.known_welcome_ids.clone(),
                    this.context.clone().into_owned(),
                    welcome_envelope?,
                    *this.conversation_type,
                    *this.include_duplicated_dms,
                    this.consent_states.clone(),
                )?;
                this.welcome_syncs.spawn(future.process());
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            None => Poll::Ready(None),
        }
    }
}

impl<'a, C, Subscription> StreamConversations<'a, C, Subscription>
where
    C: XmtpSharedContext + 'static,
    C::ApiClient: 'static,
    C::Db: 'static,
    Subscription: Stream<Item = Result<WelcomeOrGroup>> + 'static,
{
    /// adds the processed welcome id to our inner hashset
    fn filter_welcome(
        mut self: Pin<&mut Self>,
        welcome: Result<ProcessWelcomeResult<C>>,
    ) -> Option<<Self as Stream>::Item> {
        let this = self.as_mut().project();
        match welcome {
            Ok(ProcessWelcomeResult::New {
                group,
                id: welcome_id,
            }) => {
                tracing::debug!(
                    group_id = hex::encode(&group.group_id),
                    "finished processing with group {}",
                    hex::encode(&group.group_id)
                );
                this.known_welcome_ids.insert(welcome_id);
                Some(Ok(group))
            }
            // we are ignoring this payload with id
            Ok(ProcessWelcomeResult::IgnoreId { id }) => {
                tracing::debug!("ignoring streamed conversation payload with welcome id {id}");
                this.known_welcome_ids.insert(id);
                None
            }
            Ok(ProcessWelcomeResult::Ignore) => {
                tracing::debug!("ignoring streamed conversation payload");
                None
            }
            Ok(ProcessWelcomeResult::NewStored {
                group,
                maybe_sequence_id,
                maybe_originator,
            }) => {
                tracing::debug!(
                    group_id = hex::encode(&group.group_id),
                    "finished processing with group {}",
                    hex::encode(&group.group_id)
                );
                if let Some(id) = maybe_sequence_id
                    && let Some(originator) = maybe_originator
                {
                    this.known_welcome_ids
                        .insert(Cursor::new(id as SequenceId, originator as OriginatorId));
                }
                Some(Ok(group))
            }
            Err(e) => Some(Err(e)),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::utils::ClientTester;
    use std::sync::Arc;

    use super::*;
    use crate::builder::ClientBuilder;
    use crate::groups::send_message_opts::SendMessageOpts;
    use crate::tester;
    use crate::utils::fixtures::{alix, bo};
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
    #[awt]
    async fn stream_welcomes(
        #[future] alix: ClientTester,
        #[future] bo: ClientTester,
        #[case] group_size: usize,
    ) {
        let mut groups = vec![];
        let mut stream = StreamConversations::new(&bo.context, None, false, None)
            .await
            .unwrap();
        for _ in 0..group_size {
            let alix_bo_group = alix.create_group(None, None).unwrap();
            groups.push(alix_bo_group.group_id.clone());
            alix_bo_group.add_members(&[bo.inbox_id()]).await.unwrap();
        }
        while !groups.is_empty() {
            let bo_received_groups = stream.next().await.unwrap().unwrap();
            let index = groups
                .iter()
                .position(|group_id| bo_received_groups.group_id == *group_id)
                .expect("group must be found");
            groups.remove(index);
        }

        assert!(groups.is_empty(), "Groups must have all been received");
    }

    #[rstest::rstest]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_sync_groups_are_not_streamed() {
        tester!(alix, sync_worker);
        let stream = alix.stream_conversations(None, false).await?;
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
    //TODO: case 2 consistently fails on timeout only in CI in webassembly
    // difficult to tell why. not able to repro locally.
    // CI might have issues with http connection limits
    #[cfg_attr(target_arch = "wasm32", ignore)]
    async fn test_dm_stream_filter(
        #[case] conversation_type: ConversationType,
        #[case] expected: &str,
    ) {
        tester!(alix);
        tester!(bo);
        let stream = alix
            .stream_conversations(Some(conversation_type), false)
            .await
            .unwrap();
        futures::pin_mut!(stream);

        alix.find_or_create_dm(bo.inbox_id().to_string(), None)
            .await
            .unwrap();

        let group = alix.create_group(None, None).unwrap();
        group.add_members(&[bo.inbox_id()]).await.unwrap();

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
        let stream = alix.stream_conversations(None, false).await.unwrap();
        futures::pin_mut!(stream);

        alix.find_or_create_dm(davon.inbox_id().to_string(), None)
            .await
            .unwrap();
        let group = stream.next().await.unwrap();
        assert!(group.is_ok());
        groups.push(group.unwrap());

        let dm = eri
            .find_or_create_dm(alix.inbox_id().to_string(), None)
            .await
            .unwrap();
        dm.add_members(&[alix.inbox_id()]).await.unwrap();
        let group = stream.next().await.unwrap();
        assert!(group.is_ok());
        groups.push(group.unwrap());

        let group = alix.create_group(None, None).unwrap();
        group.add_members(&[bo.inbox_id()]).await.unwrap();
        let group = stream.next().await.unwrap();
        assert!(group.is_ok());
        groups.push(group.unwrap());

        assert_eq!(groups.len(), 3);
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(10))]
    async fn test_self_group_creation() {
        tester!(alix);
        tester!(bo);

        let stream = alix
            .stream_conversations(Some(ConversationType::Group), false)
            .await
            .unwrap();
        futures::pin_mut!(stream);

        alix.create_group(None, None).unwrap();
        let _self_group = stream.next().await.unwrap();

        let group = bo.create_group(None, None).unwrap();
        group.add_members(&[alix.inbox_id()]).await.unwrap();
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
        tester!(alix);
        tester!(bo);

        let alix_group = alix
            .create_group_with_members(&[bo.inbox_id().to_string()], None, None)
            .await
            .unwrap();

        alix_group.remove_members(&[bo.inbox_id()]).await.unwrap();
        bo.sync_welcomes().await.unwrap();
        let stream = bo
            .stream_conversations(Some(ConversationType::Group), false)
            .await
            .unwrap();
        futures::pin_mut!(stream);
        alix_group
            .add_members(&[bo.inbox_id().to_string()])
            .await
            .unwrap();

        let group_result = stream.next().await.unwrap();
        if let Err(error) = group_result {
            panic!("Error streaming group: {:?}", error);
        }
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(15))]
    async fn test_duplicate_dm_not_streamed() {
        use xmtp_cryptography::utils::generate_local_wallet;

        let client1 = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let client2 = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let mut stream = client1.stream_conversations(None, false).await.unwrap();

        // First DM - should stream
        let dm1 = client1
            .find_or_create_dm(client2.inbox_id().to_string(), None)
            .await
            .unwrap();

        let streamed_dm1 = stream.next().await.unwrap();
        assert!(streamed_dm1.is_ok());
        assert_eq!(streamed_dm1.unwrap().group_id, dm1.group_id);

        // Create a second DM with same participants — triggers duplicate logic
        let dm2 = client2
            .find_or_create_dm(client1.inbox_id().to_string(), None)
            .await
            .unwrap();

        // Make sure it's actually a new group
        assert_ne!(dm1.group_id, dm2.group_id);

        // It should NOT appear in the stream
        let result =
            xmtp_common::time::timeout(std::time::Duration::from_millis(100), stream.next()).await;
        assert!(result.is_err(), "Duplicate DM was unexpectedly streamed");
    }

    #[rstest::rstest]
    #[case::five_dms(5)]
    #[case::onehundred_dms(100)]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(120))]
    #[awt]
    #[cfg_attr(all(feature = "d14n"), ignore)]
    async fn test_many_concurrent_dm_invites(#[future] alix: ClientTester, #[case] dms: usize) {
        let alix_inbox_id = Arc::new(alix.inbox_id().to_string());
        let mut clients = vec![];
        for _ in 0..dms {
            let client =
                Arc::new(ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await);
            clients.push(client);
        }

        let stream = alix.stream_all_messages(None, None).await.unwrap();
        for client in clients.iter().take(dms) {
            xmtp_common::task::spawn({
                let id = alix_inbox_id.clone();
                let c = client.clone();
                async move {
                    xmtp_common::time::sleep(std::time::Duration::from_millis(100)).await;
                    let dm = c.find_or_create_dm(id.as_ref(), None).await?;
                    dm.send_message(b"hi", SendMessageOpts::default()).await?;
                    Ok::<_, crate::client::ClientError>(())
                }
            });
        }
        futures::pin_mut!(stream);
        for _ in 0..dms {
            let _welcome = stream.next().await;
        }
    }
}
