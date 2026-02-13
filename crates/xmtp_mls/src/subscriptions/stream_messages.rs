#[cfg(any(test, feature = "test-utils"))]
pub mod stream_stats;
#[cfg(any(test, feature = "test-utils"))]
mod test_utils;
mod types;

#[cfg(any(test, feature = "test-utils"))]
pub use test_utils::*;

use types::GroupList;
pub use types::MessageStreamError;
use xmtp_macro::log_event;

use super::{
    Result, SubscribeError,
    process_message::{ProcessFutureFactory, ProcessMessageFuture},
};
use crate::{
    context::XmtpSharedContext,
    groups::MlsGroup,
    subscriptions::{StreamKind, process_message::ProcessedMessage},
};
use futures::Stream;
use pin_project::{pin_project, pinned_drop};
use std::{
    borrow::Cow,
    collections::VecDeque,
    future::Future,
    pin::Pin,
    task::{Poll, ready},
};
use xmtp_common::{BoxDynFuture, Event};
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_proto::types::{Cursor, GlobalCursor, OriginatorId, SequenceId};
use xmtp_proto::types::{GroupId, Topic};
use xmtp_proto::{api_client::XmtpMlsStreams, types::TopicCursor};

impl xmtp_common::RetryableError for MessageStreamError {
    fn is_retryable(&self) -> bool {
        use MessageStreamError::*;
        match self {
            NotSubscribed(_) | InvalidPayload => false,
        }
    }
}

type AddingResult<Out> = (Out, Vec<u8>, Option<Cursor>);

#[pin_project(PinnedDrop)]
pub struct StreamGroupMessages<
    'a,
    Context: Clone + XmtpSharedContext,
    Subscription,
    Factory = ProcessMessageFuture<Context>,
> {
    #[pin]
    inner: Subscription,
    #[pin]
    state: State<'a, Subscription>,
    factory: Factory,
    context: Cow<'a, Context>,
    groups: GroupList,
    add_queue: VecDeque<MlsGroup<Context>>,
    returned: Vec<Cursor>,
    got: Vec<Cursor>,
}

#[pinned_drop]
impl<'a, Context, Subscription, Factory> PinnedDrop
    for StreamGroupMessages<'a, Context, Subscription, Factory>
where
    Context: Clone + XmtpSharedContext,
{
    fn drop(self: Pin<&mut Self>) {
        log_event!(
            Event::StreamClosed,
            self.context.installation_id(),
            kind = ?StreamKind::Messages
        );
    }
}

#[pin_project(project = ProjectState)]
#[derive(Default)]
enum State<'a, Out> {
    /// State that indicates the stream is waiting on the next message from the network
    #[default]
    Waiting,
    /// State that indicates the stream is waiting on a IO/Network future to finish processing
    /// the current message before moving on to the next one
    Processing {
        #[pin]
        future: BoxDynFuture<'a, Result<ProcessedMessage>>,
        message: Cursor,
    },
    // State that indicates that the stream is adding a new group to the stream.
    Adding {
        #[pin]
        future: BoxDynFuture<'a, Result<AddingResult<Out>>>,
    },
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
        log_event!(
            Event::StreamOpened,
            context.installation_id(),
            kind = ?StreamKind::Messages
        );
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
    C::ApiClient: XmtpMlsStreams + 'static,
    C::Db: 'static,
{
    pub async fn new_owned(context: C, groups: Vec<GroupId>) -> Result<Self> {
        let f = ProcessMessageFuture::new(context.clone());
        Self::new_with_factory(Cow::Owned(context), groups, f).await
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

        // Get the last sync cursor for each group to populate seen messages
        use xmtp_db::encrypted_store::refresh_state::{EntityKind, QueryRefreshState};
        use xmtp_db::group_message::QueryGroupMessage;

        let db = context.db();
        let cursors_by_group = db.get_last_cursor_for_ids(
            &groups,
            &[EntityKind::ApplicationMessage, EntityKind::CommitMessage],
        )?;

        // Get all cursors of messages newer than last sync for each group
        // to populate seen messages
        let seen_cursors_vec = db.messages_newer_than(&cursors_by_group)?;

        let seen_cursors: std::collections::HashSet<_> = seen_cursors_vec.into_iter().collect();

        let mut topic_cursor = TopicCursor::default();
        for group_id in &groups {
            let cursor = cursors_by_group
                .get(group_id.as_slice())
                .cloned()
                .unwrap_or_default();
            topic_cursor.add(Topic::new_group_message(group_id.clone()), cursor);
        }

        let groups_list = GroupList::new(topic_cursor, seen_cursors.clone());

        let subscription = api
            .subscribe_group_messages(&groups.iter().collect::<Vec<_>>())
            .await?;

        Ok(Self {
            inner: subscription,
            context,
            state: Default::default(),
            groups: groups_list,
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
        // we unconditionally notify, otherwise
        // test failures if we hit a group that is already in the stream.
        if self.groups.contains(&group.group_id) {
            tracing::debug!("group {} already in stream", hex::encode(&group.group_id));
            return;
        }
        tracing::debug!("queuing group add");
        // any other state and the group must be added to queue
        let this = self.as_mut().project();
        this.add_queue.push_back(group);
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
    /// * `groups_with_positions` - List of tuples containing group IDs and their current positions
    /// * `new_group` - ID of the new group to add
    ///
    /// # Returns
    /// * `Result<(MessagesApiSubscription<'a, C>, Vec<u8>, Option<Cursor>)>` - A tuple containing:
    ///   - The new message subscription
    ///   - The ID of the newly added group
    ///   - The cursor position for the new group (if available)
    ///
    /// # Errors
    /// May return errors if:
    /// - Creating the new subscription fails
    #[tracing::instrument(level = "trace", skip(context, new_group), fields(new_group = hex::encode(&new_group)))]
    #[allow(clippy::type_complexity)]
    async fn subscribe(
        context: Cow<'a, C>,
        topic_cursor: TopicCursor,
        new_group: Vec<u8>,
    ) -> Result<(
        MessagesApiSubscription<'a, C::ApiClient>,
        Vec<u8>,
        Option<Cursor>,
    )> {
        let stream = context
            .as_ref()
            .api()
            .subscribe_group_messages_with_cursors(&topic_cursor)
            .await?;
        Ok((
            stream,
            new_group,
            Some(Cursor::new(1 as SequenceId, 0 as OriginatorId)),
        ))
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
        let mut this = self.as_mut();
        match this.as_mut().project().state.as_mut().project() {
            Waiting => {
                tracing::trace!("stream messages in waiting state");
                let this = self.as_mut().project();
                if let Some(group) = this.add_queue.pop_front() {
                    self.as_mut().resolve_group_additions(group);
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                let r = self.as_mut().on_waiting(cx);
                if self.as_mut().current_state() != "waiting" {
                    tracing::trace!(
                        "stream messages returning from waiting state, transitioning to {}",
                        self.as_mut().current_state()
                    );
                }
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
                if let Ok((stream, group, cursor)) = ready!(future.poll(cx)) {
                    if let Some(c) = cursor {
                        this.as_mut().set_cursor(group.as_slice(), c)
                    };
                    this.as_mut().project().inner.set(stream);
                    let position = this.groups.position(&group);
                    tracing::debug!(
                        "added group_id={} at cursor={} to messages stream",
                        hex::encode(&group),
                        position
                    );
                }
                this.project().state.as_mut().set(State::Waiting);
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
        let Some(next_msg) = next_msg else {
            return Poll::Ready(None);
        };
        let next_msg = next_msg?;
        // ensure we have not tried processing this message yet
        // if we have tried to process, replay messages up to the known cursor.
        if self.groups.has_seen(next_msg.cursor) {
            tracing::warn!(
                "msg @cursor[{}] for group_id@[{}] has been seen, skipping.",
                next_msg.cursor,
                next_msg.group_id
            );
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        if let Some(stored) = self.factory.retrieve(&next_msg)? {
            tracing::debug!(
                "msg @cursor[{:?}] for group_id@[{}] is available locally",
                next_msg.cursor,
                next_msg.group_id
            );
            let this = self.as_mut().project();
            this.groups.set(next_msg.group_id, next_msg.cursor);
            return Poll::Ready(Some(Ok(stored)));
        }
        tracing::info!(
            "group_id@[{}] encountered newly unprocessed message @cursor=[{}]",
            next_msg.group_id,
            next_msg.cursor
        );
        let future = self.factory.create(next_msg.clone());
        let msg_cursor = next_msg.cursor;
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
        this.groups.add(&group.group_id, GlobalCursor::default());
        let groups_with_positions = self.groups.groups_with_positions().clone();
        let future = Self::subscribe(self.context.clone(), groups_with_positions, group.group_id);
        let mut this = self.as_mut().project();

        this.state.set(State::Adding {
            future: Box::pin(future),
        });
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
    ) -> Poll<Option<Result<xmtp_proto::types::GroupMessage>>> {
        let this = self.as_mut().project();
        if let Some(envelope) = ready!(this.inner.poll_next(cx)) {
            let envelope = envelope.map_err(|e| SubscribeError::BoxError(Box::new(e)))?;
            this.got.push(envelope.cursor);
            tracing::trace!(
                "got new message for group=[{}] @cursor=[{}] from network, total messages=[{}]",
                xmtp_common::fmt::debug_hex(&envelope.group_id),
                envelope.cursor,
                this.got.len()
            );
            Poll::Ready(Some(Ok(envelope)))
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
            let processed = ready!(future.poll(cx))
                .inspect_err(|_| self.as_mut().project().state.set(State::Waiting))?;
            tracing::trace!(
                "message @cursor=[{}] finished processing",
                processed.tried_to_process
            );
            let this = self.as_mut().project();
            if let Some(msg) = processed.message {
                this.returned.push(Cursor::new(
                    msg.sequence_id as SequenceId,
                    msg.originator_id as OriginatorId,
                ));
                self.as_mut()
                    .set_cursor(msg.group_id.as_slice(), processed.next_message);
                tracing::trace!(
                    "returning new message for group=[{}] @cursor=[{:?}], total messages={}",
                    xmtp_common::fmt::debug_hex(msg.group_id.as_slice()),
                    processed.tried_to_process,
                    self.returned.len()
                );
                self.as_mut().project().state.set(State::Waiting);
                return Poll::Ready(Some(Ok(msg)));
            } else {
                self.as_mut()
                    .set_cursor(processed.group_id.as_slice(), processed.next_message);
                tracing::trace!(
                    "skipping message for group=[{}] @cursor=[{}], setting cursor to [{:?}]",
                    xmtp_common::fmt::debug_hex(&processed.group_id),
                    processed.tried_to_process,
                    processed.next_message
                );
                self.as_mut().project().state.set(State::Waiting);
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
    fn set_cursor(mut self: Pin<&mut Self>, group_id: &[u8], new_cursor: Cursor) {
        let this = self.as_mut().project();
        this.groups.set(group_id, new_cursor);
    }
}

#[cfg(test)]
pub mod tests {
    use futures::stream::StreamExt;

    use crate::assert_msg;
    use crate::groups::send_message_opts::SendMessageOpts;
    use crate::tester;
    use rstest::*;

    #[rstest]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(30))]
    #[cfg_attr(target_arch = "wasm32", ignore)]
    async fn test_stream_messages() {
        tester!(alice, with_name: "alice");
        tester!(bob, with_name: "bob");

        let alice_group = alice.create_group(None, None).unwrap();
        tracing::info!("Group Id = [{}]", hex::encode(&alice_group.group_id));

        alice_group.add_members(&[bob.inbox_id()]).await.unwrap();
        let bob_groups = bob.sync_welcomes().await.unwrap();
        let bob_group = bob_groups.first().unwrap();
        alice_group.sync().await.unwrap();

        let stream = alice_group.stream().await.unwrap();
        futures::pin_mut!(stream);
        bob_group
            .send_message(b"hello", SendMessageOpts::default())
            .await
            .unwrap();

        // group updated msg/bob is added
        // assert_msg_exists!(stream);
        assert_msg!(stream, "hello");

        bob_group
            .send_message(b"hello2", SendMessageOpts::default())
            .await
            .unwrap();
        assert_msg!(stream, "hello2");
    }
}
