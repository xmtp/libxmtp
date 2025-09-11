#[cfg(test)]
mod test_case_builder;
#[cfg(test)]
mod test_utils;
#[cfg(test)]
mod unit_tests;

mod types;

use types::GroupList;
pub(super) use types::MessagePosition;
pub use types::MessageStreamError;

use std::{
    borrow::Cow,
    collections::VecDeque,
    future::Future,
    pin::Pin,
    task::{Poll, ready},
};

use super::{
    Result, SubscribeError,
    process_message::{ProcessFutureFactory, ProcessMessageFuture},
};
use crate::{
    context::XmtpSharedContext, groups::MlsGroup, subscriptions::process_message::ProcessedMessage,
};
use futures::Stream;
use pin_project_lite::pin_project;
use xmtp_api::GroupFilter;
use xmtp_common::FutureWrapper;
use xmtp_common::types::GroupId;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_proto::{
    api_client::XmtpMlsStreams,
    xmtp::mls::api::v1::{GroupMessage, group_message},
};

impl xmtp_common::RetryableError for MessageStreamError {
    fn is_retryable(&self) -> bool {
        use MessageStreamError::*;
        match self {
            NotSubscribed(_) | InvalidPayload => false,
        }
    }
}

pub fn extract_message_v1(message: GroupMessage) -> Option<group_message::V1> {
    match message.version {
        Some(group_message::Version::V1(value)) => Some(value),
        _ => None,
    }
}

pub fn extract_message_cursor(message: &GroupMessage) -> Option<u64> {
    match &message.version {
        Some(group_message::Version::V1(value)) => Some(value.id),
        _ => None,
    }
}

pin_project! {
    pub struct StreamGroupMessages<'a, Context: Clone, Subscription, Factory = ProcessMessageFuture<Context>> {
        #[pin] inner: Subscription,
        #[pin] state: State<'a, Subscription>,
        factory: Factory,
        context: Cow<'a, Context>,
        groups: GroupList,
        add_queue: VecDeque<MlsGroup<Context>>,
        returned: Vec<u64>,
        got: Vec<u64>
    }
}

pin_project! {
    #[project = ProjectState]
    #[derive(Default)]
    enum State<'a, Out> {
        /// State that indicates the stream is waiting on the next message from the network
        #[default]
        Waiting,
        /// state that indicates the stream is waiting on a IO/Network future to finish processing
        /// the current message before moving on to the next one
        Processing {
            #[pin] future: FutureWrapper<'a, Result<ProcessedMessage>>,
            message: u64
        },
        Adding {
            #[pin] future: FutureWrapper<'a, Result<(Out, Vec<u8>, Option<u64>)>>
        }
    }
}

pub(super) type MessagesApiSubscription<'a, ApiClient> =
    <ApiClient as XmtpMlsStreams>::GroupMessageStream;

impl<'a, Context> StreamGroupMessages<'a, Context, MessagesApiSubscription<'a, Context::ApiClient>>
where
    Context: XmtpSharedContext + 'a,
    Context::ApiClient: XmtpMlsStreams + 'a,
{
    /// Creates a new stream for receiving group messages.
    ///
    /// Initializes a subscription to messages for the specified groups
    ///
    /// # Arguments
    /// * `context` - Reference to the local context
    /// * `groups` - List of group IDs to subscribe to
    ///
    /// # Returns
    /// * `Result<Self>` - A new message stream if successful, or an error if initialization fails
    ///
    /// # Errors
    /// May return errors if:
    /// - Querying the latest messages fails
    /// - Message extraction fails
    /// - Creating the subscription fails
    pub async fn new(context: &'a Context, groups: Vec<GroupId>) -> Result<Self> {
        Self::new_with_factory(
            Cow::Borrowed(context),
            groups,
            ProcessMessageFuture::new(context.clone()),
        )
        .await
    }

    pub async fn from_cow(context: Cow<'a, Context>, groups: Vec<GroupId>) -> Result<Self> {
        Self::new_with_factory(
            context.clone(),
            groups,
            ProcessMessageFuture::new(context.as_ref().clone()),
        )
        .await
    }
}

impl<C> StreamGroupMessages<'static, C, MessagesApiSubscription<'static, C::ApiClient>>
where
    C: XmtpSharedContext + 'static,
    C::ApiClient: XmtpMlsStreams + Send + Sync + 'static,
    C::Db: Send + 'static,
{
    pub async fn new_owned(context: C, groups: Vec<GroupId>) -> Result<Self> {
        let f = ProcessMessageFuture::new(context.clone());
        Self::new_with_factory(Cow::Owned(context), groups, f).await
    }
}

#[cfg(test)]
impl<'a, Context: Clone, S> StreamGroupMessages<'a, Context, S> {
    pub fn position(&self, group: impl AsRef<[u8]>) -> Option<MessagePosition> {
        self.groups.position(group)
    }
}

impl<'a, C, Factory> StreamGroupMessages<'a, C, MessagesApiSubscription<'a, C::ApiClient>, Factory>
where
    C: XmtpSharedContext + 'a,
    C::ApiClient: XmtpMlsStreams + 'a,
    Factory: ProcessFutureFactory<'a> + 'a,
{
    pub async fn new_with_factory(
        context: Cow<'a, C>,
        groups: Vec<GroupId>,
        factory: Factory,
    ) -> Result<Self> {
        tracing::debug!("setting up messages subscription");
        let api = context.api();
        let groups = GroupList::new(groups, context.as_ref())?;
        let filters = groups.filters();
        let subscription = api.subscribe_group_messages(filters.clone()).await?;
        tracing::info!("stream_messages ready. Filters: {:?}", filters);

        Ok(Self {
            inner: subscription,
            context,
            state: Default::default(),
            groups,
            got: Default::default(),
            returned: Default::default(),
            add_queue: Default::default(),
            factory,
        })
    }

    /// Adds a new group to the existing message stream.
    ///
    /// This method allows dynamically extending the subscription to include
    /// messages from an additional group without recreating the entire stream.
    ///
    /// The process involves:
    /// 1. Checking if the group is already part of the stream
    /// 2. Adding the group to the tracking list
    /// 3. Re-establishing the subscription with the updated group list
    ///
    /// # Arguments
    /// * `group` - The MLS group to add to the stream
    ///
    /// # Note
    /// This is an asynchronous operation that transitions the stream to the `Adding` state.
    /// The actual subscription update happens when the stream is polled.
    pub(super) fn add(mut self: Pin<&mut Self>, group: MlsGroup<C>) {
        if self.groups.contains(&group.group_id) {
            tracing::debug!("group {} already in stream", hex::encode(&group.group_id));
            return;
        }

        // if we're waiting, resolve it right away
        if let State::Waiting = self.state {
            self.resolve_group_additions(group);
        } else {
            tracing::debug!("stream busy, queuing group add");
            // any other state and the group must be added to queue
            let this = self.as_mut().project();
            this.add_queue.push_back(group);
        }
    }

    /// Internal API to re-subscribe to a message stream.
    /// Re-subscribes to the message stream with an updated group list.
    ///
    /// Creates a new subscription that includes the specified new group,
    /// while maintaining existing subscriptions for other groups.
    ///
    /// This function:
    /// 1. Determines the appropriate cursor position for the new group
    /// 2. Updates filters for all groups
    /// 3. Establishes a new subscription with the updated filters
    ///
    /// # Arguments
    /// * `context` - Reference to the client used for API communication
    /// * `filters` - Current list of group filters
    /// * `new_group` - ID of the new group to add
    ///
    /// # Returns
    /// * `Result<(MessagesApiSubscription<'a, C>, Vec<u8>, Option<u64>)>` - A tuple containing:
    ///   - The new message subscription
    ///   - The ID of the newly added group
    ///   - The cursor position for the new group (if available)
    ///
    /// # Errors
    /// May return errors if:
    /// - Querying the database for the last cursor fails
    /// - Creating the new subscription fails
    #[tracing::instrument(level = "trace", skip(context, new_group), fields(new_group = hex::encode(&new_group)))]
    #[allow(clippy::type_complexity)]
    async fn subscribe(
        context: Cow<'a, C>,
        filters: Vec<GroupFilter>,
        new_group: Vec<u8>,
    ) -> Result<(
        MessagesApiSubscription<'a, C::ApiClient>,
        Vec<u8>,
        Option<u64>,
    )> {
        // get the last synced cursor
        let stream = context.api().subscribe_group_messages(filters).await?;
        Ok((stream, new_group, Some(1)))
    }
}

impl<'a, C, Factory> Stream
    for StreamGroupMessages<'a, C, MessagesApiSubscription<'a, C::ApiClient>, Factory>
where
    C: XmtpSharedContext + 'a,
    C::ApiClient: XmtpMlsStreams + 'a,
    Factory: ProcessFutureFactory<'a> + 'a,
{
    type Item = Result<StoredGroupMessage>;

    #[tracing::instrument(level = "trace", skip_all, name = "poll_next_message")]
    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        use ProjectState::*;
        let mut this = self.as_mut().project();
        let state = this.state.as_mut().project();
        match state {
            Waiting => {
                tracing::trace!("stream messages in waiting state");
                if let Some(group) = this.add_queue.pop_front() {
                    self.as_mut().resolve_group_additions(group);
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                let r = self.as_mut().on_waiting(cx);
                tracing::trace!(
                    "stream messages returning from waiting state, transitioning to {}",
                    self.as_mut().current_state()
                );
                r
            }
            Processing { message, .. } => {
                tracing::trace!(
                    "stream messages in processing state. Processing future for envelope @cursor=[{}]",
                    message
                );
                let r = self.as_mut().resolve_futures(cx);
                match r {
                    Poll::Ready(Some(_)) => {
                        tracing::trace!(
                            "stream messages returning from processing state, transitioning to {} state, ready with item",
                            self.as_mut().current_state()
                        )
                    }
                    Poll::Ready(None) => {
                        tracing::trace!(
                            "stream messages returning from processing state, Ready with None"
                        )
                    }
                    _ => (),
                }
                r
            }
            Adding { future } => {
                tracing::trace!("stream messages in adding state");
                let (stream, group, cursor) = ready!(future.poll(cx))?;
                let this = self.as_mut();
                if let Some(c) = cursor {
                    this.set_cursor(group.as_slice(), c)
                };
                let mut this = self.as_mut().project();
                this.inner.set(stream);
                if let Some(cursor) = this.groups.position(&group) {
                    tracing::debug!(
                        "added group_id={} at cursor={} to messages stream",
                        hex::encode(&group),
                        cursor
                    );
                }
                this.state.as_mut().set(State::Waiting);
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
}

impl<'a, C, Factory> StreamGroupMessages<'a, C, MessagesApiSubscription<'a, C::ApiClient>, Factory>
where
    C: XmtpSharedContext + 'a,
    Factory: ProcessFutureFactory<'a> + 'a,
    C::ApiClient: XmtpMlsStreams + 'a,
{
    /// Get the current state of the stream as a [`String`]
    fn current_state(self: Pin<&mut Self>) -> String {
        match self.as_ref().state {
            State::Waiting { .. } => "waiting".into(),
            State::Processing { .. } => "processing".into(),
            State::Adding { .. } => "adding".into(),
        }
    }

    /// Handles the stream when in the `Waiting` state.
    ///
    /// This method is called when the stream is ready to process the next message.
    /// It:
    /// 1. Waits for the next message from the inner stream
    /// 2. Checks if the message has already been processed by comparing cursors
    /// 3. Either processes the message or transitions to replay mode if needed
    ///
    /// # Arguments
    /// * `cx` - The task context for polling
    ///
    /// # Returns
    /// * `Poll<Option<Result<StoredGroupMessage>>>` - The polling result:
    ///   - `Ready(Some(Ok(msg)))` if a message is successfully processed
    ///   - `Ready(None)` if the stream is terminated
    ///   - `Pending` if waiting for more data
    #[tracing::instrument(level = "trace", skip_all)]
    fn on_waiting(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<<Self as Stream>::Item>> {
        let next_msg = ready!(self.as_mut().next_message(cx));
        if next_msg.is_none() {
            return Poll::Ready(None);
        }
        let mut next_msg = next_msg.expect("checked for none")?;
        // ensure we have not tried processing this message yet
        // if we have tried to process, replay messages up to the known cursor.
        let cursor = self.groups.position(&next_msg.group_id);
        if let Some(position) = cursor {
            if position.last_streamed() > next_msg.id {
                tracing::warn!(
                    "stream started @[{}] has cursor@[{}] for group_id@[{}], skipping messages for msg with cursor@[{}]",
                    position.started(),
                    position.last_streamed(),
                    xmtp_common::fmt::truncate_hex(hex::encode(next_msg.group_id.as_slice())),
                    next_msg.id,
                );
                next_msg = ready!(self.as_mut().skip(cx, next_msg))?;
            }
            tracing::debug!(
                "stream @cursor=[{}] for group_id@[{}] encountered newly unprocessed message @cursor=[{}]",
                position.last_streamed(),
                xmtp_common::fmt::debug_hex(next_msg.group_id.as_slice()),
                next_msg.id
            );
        }
        let future = self.factory.create(next_msg.clone());
        let msg_cursor = next_msg.id;
        let mut this = self.as_mut().project();
        this.state.set(State::Processing {
            future,
            message: msg_cursor,
        });
        cx.waker().wake_by_ref();
        Poll::Pending
    }

    /// Add the group to the group list
    /// and transition the stream to Adding state
    fn resolve_group_additions(mut self: Pin<&mut Self>, group: MlsGroup<C>) {
        tracing::debug!(
            "begin establishing new message stream to include group_id={}",
            hex::encode(&group.group_id)
        );
        let this = self.as_mut().project();
        this.groups.add(&group.group_id, MessagePosition::new(1, 1));
        let future = Self::subscribe(self.context.clone(), self.groups.filters(), group.group_id);
        let mut this = self.as_mut().project();
        this.state.set(State::Adding {
            future: FutureWrapper::new(future),
        });
    }

    // iterative skip to avoid overflowing the stack
    fn skip(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        mut envelope: group_message::V1,
    ) -> Poll<Result<group_message::V1>> {
        // skip the messages
        while let Some(new_envelope) = ready!(self.as_mut().next_message(cx)) {
            let new_envelope = new_envelope?;
            if let Some(stream_cursor) = self.as_ref().groups.position(&new_envelope.group_id) {
                if stream_cursor.last_streamed() > new_envelope.id {
                    tracing::info!(
                        "skipping msg with group_id@[{}] and cursor@[{}]",
                        xmtp_common::fmt::debug_hex(new_envelope.group_id.as_slice()),
                        new_envelope.id
                    );
                    continue;
                }
            } else {
                envelope = new_envelope;
                tracing::trace!("finished skipping");
                break;
            }
        }
        Poll::Ready(Ok(envelope))
    }

    /// Retrieves the next message from the inner stream.
    ///
    /// Polls the underlying subscription for the next message and extracts
    /// the V1 payload if available.
    ///
    /// # Arguments
    /// * `cx` - The task context for polling
    ///
    /// # Returns
    /// * `Poll<Option<Result<group_message::V1>>>` - The polling result:
    ///   - `Ready(Some(Ok(msg)))` if a valid message is available
    ///   - `Ready(None)` if the stream is terminated
    ///   - `Pending` if waiting for more data
    ///
    /// # Errors
    /// Returns an error if:
    /// - The inner stream returns an error
    /// - The message cannot be extracted (unsupported version)
    #[tracing::instrument(level = "trace", skip_all)]
    fn next_message(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Result<group_message::V1>>> {
        let this = self.as_mut().project();
        if let Some(envelope) = ready!(this.inner.poll_next(cx)) {
            let envelope = envelope.map_err(|e| SubscribeError::BoxError(Box::new(e)))?;
            if let Some(msg) = extract_message_v1(envelope) {
                this.got.push(msg.id);
                tracing::info!(
                    "got new message for group=[{}] @cursor=[{}] from network, total messages=[{}]",
                    xmtp_common::fmt::debug_hex(&msg.group_id),
                    msg.id,
                    this.got.len()
                );
                Poll::Ready(Some(Ok(msg)))
            } else {
                tracing::error!("bad message");
                // _NOTE_: This would happen if we receive a message
                // with a version not supported by the current client.
                // A version we don't know how to deserialize will return 'None'.
                // In this case the unreadable message would be skipped.
                self.next_message(cx)
            }
        } else {
            Poll::Ready(None)
        }
    }

    /// Resolves futures when the stream is in the `Processing` state.
    ///
    /// This method handles the completion of asynchronous operations:
    /// - When a message is processed, updates the cursor and yields the message
    /// - When no message is available, updates the cursor and continues polling
    /// - When in replay mode, delegates to `resolve_replaying`
    ///
    /// # Arguments
    /// * `cx` - The task context for polling
    ///
    /// # Returns
    /// * `Poll<Option<Result<StoredGroupMessage>>>` - The polling result based on
    ///   the current state and operation outcome
    #[tracing::instrument(level = "trace", skip_all)]
    fn resolve_futures(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<<Self as Stream>::Item>> {
        use ProjectState::*;
        if let Processing { future, .. } = self.as_mut().project().state.project() {
            let processed = ready!(future.poll(cx))?;
            tracing::trace!(
                "message @cursor=[{}] finished processing",
                processed.tried_to_process
            );
            let mut this = self.as_mut().project();
            if let Some(msg) = processed.message {
                this.state.set(State::Waiting);
                this.returned
                    .push(msg.sequence_id.map(|s| s as u64).unwrap_or(0u64));
                self.as_mut()
                    .set_cursor(msg.group_id.as_slice(), processed.next_message);
                tracing::trace!(
                    "returning new message for group=[{}] @cursor=[{:?}], total messages={}",
                    xmtp_common::fmt::debug_hex(msg.group_id.as_slice()),
                    processed.tried_to_process,
                    self.returned.len()
                );
                return Poll::Ready(Some(Ok(msg)));
            } else {
                this.state.set(State::Waiting);
                self.as_mut()
                    .set_cursor(processed.group_id.as_slice(), processed.next_message);
                tracing::trace!(
                    "skipping message for group=[{}] @cursor=[{}], setting cursor to [{:?}]",
                    xmtp_common::fmt::debug_hex(&processed.group_id),
                    processed.tried_to_process,
                    processed.next_message
                );
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        }
        Poll::Pending
    }

    /// Updates the cursor position for a specific group.
    ///
    /// This method updates the tracking information for a group after
    /// successfully processing a message, allowing the stream to maintain
    /// proper ordering and prevent duplicate processing.
    ///
    /// # Arguments
    /// * `group_id` - The ID of the group to update
    /// * `new_cursor` - The new cursor position to set
    fn set_cursor(mut self: Pin<&mut Self>, group_id: &[u8], new_cursor: u64) {
        let this = self.as_mut().project();
        this.groups.set(group_id, new_cursor);

        // Update shared mapping
        self.context
            .update_shared_last_streamed(group_id, new_cursor);
    }
}

#[cfg(test)]
pub mod tests {
    use std::sync::Arc;

    // use futures::stream::StreamExt;
    use tokio_stream::StreamExt;

    use crate::assert_msg;
    use crate::builder::ClientBuilder;
    use crate::groups::MlsGroup;
    use crate::utils::TestXmtpMlsContext;
    use rstest::*;
    use xmtp_cryptography::utils::generate_local_wallet;

    // Creates both sides of a conversation, with both sides synced and up to date
    async fn setup_groups() -> (MlsGroup<TestXmtpMlsContext>, MlsGroup<TestXmtpMlsContext>) {
        let amal = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let amal_group = amal
            .create_group_with_inbox_ids(&[bola.inbox_id()], None, None)
            .await
            .unwrap();

        bola.sync_welcomes().await.unwrap();
        let bola_group = bola.group(&amal_group.group_id).unwrap();
        bola_group.sync().await.unwrap();

        (amal_group, bola_group)
    }

    #[rstest]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(5))]
    #[cfg_attr(target_arch = "wasm32", ignore)]
    async fn test_stream_messages() {
        let (amal_group, bola_group) = setup_groups().await;

        let stream = amal_group.stream().await.unwrap();
        futures::pin_mut!(stream);

        bola_group.send_message(b"hello").await.unwrap();

        // group updated msg/bob is added
        // assert_msg_exists!(stream);
        assert_msg!(stream, "hello");

        bola_group.send_message(b"hello2").await.unwrap();
        assert_msg!(stream, "hello2");
    }

    #[rstest]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(5))]
    #[cfg_attr(target_arch = "wasm32", ignore)]
    async fn test_stream_covers_missing_messages() {
        let (amal_group, bola_group) = setup_groups().await;
        bola_group.send_message(b"hello").await.unwrap();
        bola_group.send_message(b"hello2").await.unwrap();
        bola_group.send_message(b"hello3").await.unwrap();
        bola_group.send_message(b"hello4").await.unwrap();
        bola_group.send_message(b"hello5").await.unwrap();

        let stream = amal_group.stream().await.unwrap();
        futures::pin_mut!(stream);

        assert_msg!(stream, "hello");
        assert_msg!(stream, "hello2");
        assert_msg!(stream, "hello3");
        assert_msg!(stream, "hello4");
        assert_msg!(stream, "hello5");
    }

    #[rstest]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(5))]
    #[cfg_attr(target_arch = "wasm32", ignore)]
    async fn test_stream_interrupt_and_resume() {
        let (amal_group, bola_group) = setup_groups().await;
        {
            let stream = amal_group.stream().await.unwrap();
            futures::pin_mut!(stream);

            bola_group.send_message(b"hello from bola").await.unwrap();
            // Ensure the group updated message is streamed
            assert_msg!(stream, "hello from bola");
        }
        {
            let stream = amal_group.stream().await.unwrap();
            futures::pin_mut!(stream);

            // Send a message
            bola_group.send_message(b"hello again").await.unwrap();
            // Ensure that the new message is the first result from the stream...not the group updated
            assert_msg!(stream, "hello again");

            // Send a second message
            bola_group.send_message(b"hello 3").await.unwrap();
            assert_msg!(stream, "hello 3");
        }
    }

    #[rstest]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(5))]
    #[cfg_attr(target_arch = "wasm32", ignore)]
    async fn test_stream_after_last_sync() {
        let (amal_group, bola_group) = setup_groups().await;

        bola_group.send_message(b"hello").await.unwrap();
        // Syncing the group will update the cursor so the "hello" message is not returned
        amal_group.sync().await.unwrap();

        // The stream should only return messages after the last sync
        let stream = amal_group.stream().await.unwrap();
        futures::pin_mut!(stream);

        bola_group.send_message(b"hello2").await.unwrap();
        assert_msg!(stream, "hello2");
    }
}
