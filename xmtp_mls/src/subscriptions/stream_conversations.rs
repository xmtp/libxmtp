use super::{LocalEvents, Result, SubscribeError};
use crate::{
    groups::{scoped_client::ScopedGroupClient, MlsGroup},
    subscriptions::process_welcome::ProcessWelcomeFuture,
    Client,
};
use xmtp_db::{group::ConversationType, refresh_state::EntityKind, XmtpDb};

use futures::{prelude::stream::Select, Stream};
use pin_project_lite::pin_project;
use std::{
    collections::HashSet,
    future::Future,
    pin::Pin,
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
    /// * `Poll<Option<Result<MlsGroup<C>>>>` - The polling result:
    ///   - `Ready(Some(Ok(group)))` when a group is successfully processed
    ///   - `Ready(Some(Err(e)))` when an error occurs
    ///   - `Pending` when waiting for more data or for future completion
    ///   - `Ready(None)` when the stream has ended
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
    /// * `Poll<Option<Result<MlsGroup<C>>>>` - The polling result based on
    ///   the welcome processing outcome
    ///
    /// # Note
    /// This method is critical for maintaining the stream's state machine and
    /// ensuring proper handling of all possible processing outcomes.
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
