#[cfg(test)]
mod test_case_builder;
#[cfg(test)]
mod test_utils;

use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
};

use super::{
    process_message::{ProcessFutureFactory, ProcessMessageFuture},
    Result, SubscribeError,
};
use crate::{
    context::{XmtpContextProvider, XmtpMlsLocalContext},
    groups::MlsGroup,
    subscriptions::process_message::ProcessedMessage,
};
use futures::Stream;
use pin_project_lite::pin_project;
use xmtp_api::GroupFilter;
use xmtp_common::types::GroupId;
use xmtp_common::FutureWrapper;
use xmtp_db::{group_message::StoredGroupMessage, XmtpDb};
use xmtp_proto::{
    api_client::{trait_impls::XmtpApi, XmtpMlsStreams},
    xmtp::mls::api::v1::{group_message, GroupMessage},
};

#[derive(thiserror::Error, Debug)]
pub enum MessageStreamError {
    #[error("received message for not subscribed group {id}", id = hex::encode(_0))]
    NotSubscribed(Vec<u8>),
    #[error("Invalid Payload")]
    InvalidPayload,
}

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

/// the position of this message in the backend topic
/// based only upon messages from the stream
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessagePosition {
    started_at: u64,
    /// the time we last synced the group
    /// If we get a message before this cursor, we should not require
    /// syncing, and should prefer going to the database
    last_synced: Option<u64>,
}

impl MessagePosition {
    pub fn new(cursor: u64, started_at: u64) -> Self {
        Self {
            last_synced: Some(cursor),
            started_at,
        }
    }
    /// Updates the cursor position for this message.
    ///
    /// Sets the cursor to a specific position in the message stream, which
    /// helps track which messages have been processed.
    ///
    /// # Arguments
    /// * `cursor` - The new cursor position to set
    pub(super) fn set(&mut self, cursor: u64) {
        self.last_synced = Some(cursor);
    }

    /// Retrieves the current cursor position.
    ///
    /// Returns the cursor position or 0 if no cursor has been set yet.
    ///
    /// # Returns
    /// * `u64` - The current cursor position or 0 if unset
    pub(crate) fn pos(&self) -> u64 {
        self.last_synced.unwrap_or(0)
    }

    /// when did the stream start streaming for this group
    pub(crate) fn started(&self) -> u64 {
        self.started_at
    }
}

impl std::fmt::Display for MessagePosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.pos())
    }
}

pin_project! {
    pub struct StreamGroupMessages<'a, ApiClient, Db, Subscription, Factory = ProcessMessageFuture<ApiClient, Db>> {
        #[pin] inner: Subscription,
        #[pin] state: State<'a, Subscription>,
        factory: Factory,
        context: &'a Arc<XmtpMlsLocalContext<ApiClient, Db>>,
        pub(super) group_list: HashMap<GroupId, MessagePosition>,
        add_queue: VecDeque<MlsGroup<ApiClient, Db>>,
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
    <ApiClient as XmtpMlsStreams>::GroupMessageStream<'a>;

impl<'a, ApiClient, Db>
    StreamGroupMessages<'a, ApiClient, Db, MessagesApiSubscription<'a, ApiClient>>
where
    ApiClient: XmtpApi + XmtpMlsStreams + 'a,
    Db: XmtpDb + 'a,
{
    /// Creates a new stream for receiving group messages.
    ///
    /// Initializes a subscription to messages for the specified groups, tracking
    /// cursor positions to ensure proper message ordering and prevent duplicates.
    ///
    /// This function:
    /// 1. Queries the latest message for each group to establish initial cursor positions
    /// 2. Creates filters for each group based on these cursor positions
    /// 3. Sets up the subscription to the message stream
    ///
    /// # Arguments
    /// * `client` - Reference to the client used for API communication
    /// * `group_list` - List of group IDs to subscribe to
    ///
    /// # Returns
    /// * `Result<Self>` - A new message stream if successful, or an error if initialization fails
    ///
    /// # Errors
    /// May return errors if:
    /// - Querying the latest messages fails
    /// - Message extraction fails
    /// - Creating the subscription fails
    pub async fn new(
        context: &'a Arc<XmtpMlsLocalContext<ApiClient, Db>>,
        group_list: Vec<GroupId>,
    ) -> Result<Self> {
        Self::new_with_factory(
            context,
            group_list,
            ProcessMessageFuture::new(context.clone()),
        )
        .await
    }
}

impl<'a, ApiClient, Db, Factory>
    StreamGroupMessages<'a, ApiClient, Db, MessagesApiSubscription<'a, ApiClient>, Factory>
where
    ApiClient: XmtpApi + XmtpMlsStreams + 'a,
    Db: XmtpDb + 'a,
    Factory: ProcessFutureFactory<'a> + 'a,
{
    pub async fn new_with_factory(
        context: &'a Arc<XmtpMlsLocalContext<ApiClient, Db>>,
        group_list: Vec<GroupId>,
        factory: Factory,
    ) -> Result<Self> {
        tracing::debug!("setting up messages subscription");

        let mut group_list = group_list
            .into_iter()
            .map(|group_id| (group_id, 0u64))
            .collect::<HashMap<GroupId, u64>>();
        let api = context.api();
        let cursors = group_list
            .keys()
            .map(|group| api.query_latest_group_message(group));

        let cursors = futures::future::join_all(cursors)
            .await
            .into_iter()
            .map(|r| r.map_err(SubscribeError::from))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        for message in cursors {
            let group_message::V1 {
                id: cursor,
                group_id,
                ..
            } = extract_message_v1(message).ok_or(MessageStreamError::InvalidPayload)?;
            group_list
                .entry(group_id.clone().into())
                .and_modify(|e| *e = cursor);
        }

        let filters: Vec<GroupFilter> = group_list
            .iter()
            .inspect(|(group_id, cursor)| {
                tracing::debug!(
                    "subscribed to group {} at {}",
                    xmtp_common::fmt::truncate_hex(hex::encode(group_id)),
                    cursor
                )
            })
            .map(|(group_id, cursor)| GroupFilter::new(group_id.to_vec(), Some(*cursor)))
            .collect();
        let subscription = api.subscribe_group_messages(filters).await?;
        let group_list = group_list
            .into_iter()
            .map(|(g, c)| (g, MessagePosition::new(c, c)));
        tracing::info!("stream_messages ready");

        Ok(Self {
            inner: subscription,
            context,
            state: Default::default(),
            group_list: group_list.collect(),
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
    pub(super) fn add(mut self: Pin<&mut Self>, group: MlsGroup<ApiClient, Db>) {
        if self.group_list.contains_key(group.group_id.as_slice()) {
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
    /// * `client` - Reference to the client used for API communication
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
    #[tracing::instrument(level = "trace", skip(client, new_group), fields(new_group = hex::encode(&new_group)))]
    async fn subscribe(
        client: &'a Arc<XmtpMlsLocalContext<ApiClient, Db>>,
        filters: Vec<GroupFilter>,
        new_group: Vec<u8>,
    ) -> Result<(MessagesApiSubscription<'a, ApiClient>, Vec<u8>, Option<u64>)> {
        // get the last synced cursor
        let stream = client.api().subscribe_group_messages(filters).await?;
        Ok((stream, new_group, Some(1)))
    }
}

impl<'a, ApiClient, Db, Factory> Stream
    for StreamGroupMessages<'a, ApiClient, Db, MessagesApiSubscription<'a, ApiClient>, Factory>
where
    ApiClient: XmtpApi + XmtpMlsStreams + 'a,
    Db: XmtpDb + 'a,
    Factory: ProcessFutureFactory<'a> + 'a,
{
    type Item = Result<StoredGroupMessage>;

    #[tracing::instrument(level = "trace", skip_all, name = "poll_next_message")]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
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
                tracing::trace!("stream messages in processing state. Processing future for envelope @cursor=[{}]", message);
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
                if let Some(cursor) = this.group_list.get(group.as_slice()) {
                    tracing::debug!(
                        "added group_id={} at cursor={} to messages stream",
                        hex::encode(&group),
                        cursor
                    );
                }
                this.state.as_mut().set(State::Waiting);
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        }
    }
}

impl<Api, Db, S, F> StreamGroupMessages<'_, Api, Db, S, F> {
    fn filters(&self) -> Vec<GroupFilter> {
        self.group_list
            .iter()
            .map(|(group_id, cursor)| GroupFilter::new(group_id.to_vec(), Some(cursor.pos())))
            .collect()
    }
}

impl<'a, Api, Db, Factory>
    StreamGroupMessages<'a, Api, Db, MessagesApiSubscription<'a, Api>, Factory>
where
    Api: XmtpApi + XmtpMlsStreams + 'a,
    Db: XmtpDb + 'a,
    Factory: ProcessFutureFactory<'a> + 'a,
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
        cx: &mut Context<'_>,
    ) -> Poll<Option<<Self as Stream>::Item>> {
        let envelope = ready!(self.as_mut().next_message(cx));
        if envelope.is_none() {
            return Poll::Ready(None);
        }
        let mut envelope = envelope.expect("checked for none")?;
        // ensure we have not tried processing this message yet
        // if we have tried to process, replay messages up to the known cursor.
        let cursor = self
            .as_ref()
            .group_list
            .get(envelope.group_id.as_slice())
            .copied();
        if let Some(m) = cursor {
            // If we get a message, and its cursor is greater than what the stream started at
            // but our last synced position is greater, try to get it from the database rather
            // then starting process future
            // otherwise, skip the message
            if m.pos() > envelope.id && m.started() > envelope.id {
                tracing::warn!(
                    "stream started @[{}] has cursor@[{}] for group_id@[{}], skipping messages for msg with cursor@[{}]",
                    m.started(),
                    m.pos(),
                    xmtp_common::fmt::truncate_hex(hex::encode(
                        envelope.group_id.as_slice()
                    )),
                    envelope.id,
                );
                envelope = ready!(self.as_mut().skip(cx, envelope))?;
            // we got a message with a sequence_iud greater than a message we already processed
            // so it must be present in the database
            } else if m.pos() > envelope.id && m.started() < envelope.id {
                tracing::debug!(
                    "stream synced up to cursor@[{}], checking for message with cursor@[{}] in database",
                    m.pos(),
                    envelope.id
                );
                if let Some(stored) = self.factory.retrieve(&envelope)? {
                    return Poll::Ready(Some(Ok(stored)));
                } else {
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
            }
            tracing::debug!(
                "stream @cursor=[{}] for group_id@[{}] encountered newly unprocessed message @cursor=[{}]",
                m.pos(),
                xmtp_common::fmt::debug_hex(envelope.group_id.as_slice()),
                envelope.id
            );
        }
        let future = self.factory.create(envelope.clone());
        let msg_cursor = envelope.id;
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
    fn resolve_group_additions(mut self: Pin<&mut Self>, group: MlsGroup<Api, Db>) {
        tracing::debug!(
            "begin establishing new message stream to include group_id={}",
            hex::encode(&group.group_id)
        );
        let this = self.as_mut().project();
        this.group_list
            .insert(group.group_id.clone().into(), MessagePosition::new(1, 1));
        let future = Self::subscribe(self.context, self.filters(), group.group_id);
        let mut this = self.as_mut().project();
        this.state.set(State::Adding {
            future: FutureWrapper::new(future),
        });
    }

    // iterative skip to avoid overflowing the stack
    fn skip(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut envelope: group_message::V1,
    ) -> Poll<Result<group_message::V1>> {
        // skip the messages
        while let Some(new_envelope) = ready!(self.as_mut().next_message(cx)) {
            let new_envelope = new_envelope?;
            if let Some(stream_cursor) = self
                .as_ref()
                .group_list
                .get(new_envelope.group_id.as_slice())
                .copied()
            {
                if stream_cursor.pos() > new_envelope.id {
                    tracing::debug!(
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
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<group_message::V1>>> {
        let this = self.as_mut().project();
        if let Some(envelope) = ready!(this.inner.poll_next(cx)) {
            let envelope = envelope.map_err(|e| SubscribeError::BoxError(Box::new(e)))?;

            if let Some(msg) = extract_message_v1(envelope) {
                this.got.push(msg.id);
                tracing::trace!(
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
        cx: &mut Context<'_>,
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
                tracing::trace!(
                    "message processed, setting cursor to [{:?}] for group {}",
                    processed.next_message,
                    xmtp_common::fmt::truncate_hex(hex::encode(msg.group_id.as_slice()))
                );
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
                tracing::debug!(
                    "no message could be processed, stream setting cursor to [{:?}] for group: {}",
                    processed.next_message,
                    xmtp_common::fmt::truncate_hex(hex::encode(processed.group_id.as_slice()))
                );
                self.as_mut()
                    .set_cursor(processed.group_id.as_slice(), processed.next_message);
                tracing::trace!(
                    "skipping message for group=[{}] @cursor=[{}]",
                    xmtp_common::fmt::debug_hex(&processed.group_id),
                    processed.tried_to_process
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
        if let Some(cursor) = this.group_list.get_mut(group_id) {
            cursor.set(new_cursor);
        }
    }
}

#[cfg(test)]
pub mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::task::Poll;

    use futures::stream::StreamExt;
    use mockall::Sequence;
    use parking_lot::Mutex;
    use xmtp_api::test_utils::MockGroupStream;
    use xmtp_common::types::GroupId;
    use xmtp_common::FutureWrapper;
    use xmtp_proto::mls_v1::QueryGroupMessagesResponse;

    use super::test_utils::cases::group_list;
    use crate::assert_msg;
    use crate::subscriptions::process_message::ProcessedMessage;
    use crate::subscriptions::stream_messages::StreamGroupMessages;
    use crate::test::mock::context;
    use crate::test::mock::generate_message;
    use crate::test::mock::generate_message_and_v1;
    use crate::test::mock::generate_stored_msg;
    use crate::test::mock::MockContext;
    use crate::test::mock::MockProcessFutureFactory;
    use crate::{builder::ClientBuilder, groups::GroupMetadataOptions};
    use futures::future;
    use futures::Stream;
    use rstest::*;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_mls_common::group::GroupMetadataOptions;

    #[rstest]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(5))]
    async fn test_stream_messages() {
        let alice = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bob = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alice_group = alice
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        tracing::info!("Group Id = [{}]", hex::encode(&alice_group.group_id));

        alice_group
            .add_members_by_inbox_id(&[bob.inbox_id()])
            .await
            .unwrap();
        let bob_groups = bob.sync_welcomes().await.unwrap();
        let bob_group = bob_groups.first().unwrap();
        alice_group.sync().await.unwrap();

        let stream = alice_group.stream().await.unwrap();
        futures::pin_mut!(stream);
        bob_group.send_message(b"hello").await.unwrap();

        // group updated msg/bob is added
        // assert_msg_exists!(stream);
        assert_msg!(stream, "hello");

        bob_group.send_message(b"hello2").await.unwrap();
        assert_msg!(stream, "hello2");
    }

    // type MockStream<'a> = StreamGroupMessages<'a, MockApiClient, MockXmtpDb, MockGroupStream>;

    pub fn group_id(id: u8) -> GroupId {
        let mut v = vec![id];
        v.resize(31, 0);
        GroupId::from(v)
    }

    #[derive(Clone, Debug, Copy)]
    pub struct MessageTestCase {
        pub cursor: u64,
        pub group_id: u8,
        pub found: bool,
        pub next_cursor: u64,
        /// whether the message is retrieved from the db
        pub retrieved: bool,
        /// amnt of polls that return pending before future resolved
        pub polls_to_resolve: u8,
        pub polls_to_process: u8,
    }
    impl MessageTestCase {
        fn message(
            cursor: u64,
            group_id: u8,
            found: bool,
            next_cursor: u64,
            retrieved: bool,
            polls_to_resolve: u8,
            polls_to_process: u8,
        ) -> Self {
            MessageTestCase {
                cursor,
                group_id,
                found,
                next_cursor,
                retrieved,
                polls_to_resolve,
                polls_to_process,
            }
        }

        fn found(cursor: u64, group_id: u8, next_cursor: u64) -> Self {
            Self::message(cursor, group_id, true, next_cursor, false, 1, 1)
        }
        /*
                /// The message should be retrieved from the database
                fn retrieved(cursor: u64, group_id: u8, found: bool) -> Self {
                    Self::message(cursor, group_id, found, 9999, true, 1, 1)
                }
        */
        fn not_found(cursor: u64, group_id: u8, next_cursor: u64) -> Self {
            Self::message(cursor, group_id, false, next_cursor, false, 1, 1)
        }

        fn processing_for(cursor: u64, group_id: u8, next_cursor: u64, processing_for: u8) -> Self {
            Self::message(
                cursor,
                group_id,
                true,
                next_cursor,
                false,
                1,
                processing_for,
            )
        }
    }

    #[derive(Clone, Debug, Copy, Default)]
    pub struct GroupTestCase {
        pub group_id: u8,
    }

    #[derive(Clone, Debug)]
    pub enum StreamSession {
        Init {
            // starting groups
            groups: Vec<GroupTestCase>,
            messages: Vec<MessageTestCase>,
            //array of expected cursors
            expected: Vec<u64>,
        },
        Session {
            // groups to call add() with
            groups: Vec<GroupTestCase>,
            // messages coming after the add
            messages: Vec<MessageTestCase>,
            // array of expected cursors
            expected: Vec<u64>,
        },
    }

    impl StreamSession {
        pub fn init(groups: Vec<u8>, messages: Vec<MessageTestCase>, expected: Vec<u64>) -> Self {
            StreamSession::Init {
                groups: groups
                    .into_iter()
                    .map(|g| GroupTestCase { group_id: g })
                    .collect(),
                messages,
                expected,
            }
        }

        pub fn messages(&self) -> Vec<MessageTestCase> {
            match self {
                Self::Init { messages, .. } => messages.clone(),
                Self::Session { messages, .. } => messages.clone(),
            }
        }

        pub fn session(
            groups: Vec<u8>,
            messages: Vec<MessageTestCase>,
            expected: Vec<u64>,
        ) -> Self {
            StreamSession::Session {
                groups: groups
                    .into_iter()
                    .map(|g| GroupTestCase { group_id: g })
                    .collect(),
                messages,
                expected,
            }
        }
    }

    #[derive(Clone, Debug, Copy)]
    pub enum MessageCase {
        Message(MessageTestCase),
    }

    impl MessageCase {
        // unwrap a message test case. panics if something else
        fn unwrap_message(self) -> MessageTestCase {
            match self {
                Self::Message(m) => m,
            }
        }
    }

    impl MessageCase {
        fn message(
            cursor: u64,
            group_id: u8,
            found: bool,
            next_cursor: u64,
            retrieved: bool,
            polls_to_resolve: u8,
        ) -> Self {
            Self::Message(MessageTestCase {
                cursor,
                group_id,
                found,
                next_cursor,
                retrieved,
                polls_to_resolve,
                polls_to_process: 1,
            })
        }

        fn found(cursor: u64, group_id: u8, next_cursor: u64) -> Self {
            Self::message(cursor, group_id, true, next_cursor, false, 1)
        }

        /// The message should be retrieved from the database
        fn retrieved(cursor: u64, group_id: u8, found: bool) -> Self {
            Self::message(cursor, group_id, found, 9999, true, 1)
        }

        fn not_found(cursor: u64, group_id: u8, next_cursor: u64) -> Self {
            Self::message(cursor, group_id, false, next_cursor, false, 1)
        }
        /*
                /// Message will be processed, and it will be busy for 'busy_for' poll_next calls
                fn busy_for(cursor: u64, group_id: u8, next_cursor: u64, busy_for: u8) -> Self {
                    Self::message(cursor, group_id, true, next_cursor, false, busy_for)
                }
        */
    }

    fn setup_stream(cases: Vec<MessageTestCase>, stream: &mut MockGroupStream) {
        let mut msg_seq = Sequence::new();
        for case in &cases {
            stream
                .expect_poll_next()
                .times(case.polls_to_resolve as usize)
                .in_sequence(&mut msg_seq)
                .returning({
                    let case = *case;
                    move |_| {
                        let (msg, _) =
                            generate_message_and_v1(case.cursor, &group_id(case.group_id));
                        Poll::Ready(Some(Ok(msg)))
                    }
                });
        }

        // default value for a stream is to just end it
        // doesn't need to be called necessarily
        stream.expect_poll_next().returning(|_| Poll::Ready(None));
    }

    /// the initial test stream setup
    /// we assume the stream has 4 groups rangin in group id from 1-4
    /// each group is padded with 31 0s
    fn initial_stream_setup(context: &mut MockContext) {
        context
            .api_client
            .api_client
            .expect_query_group_messages()
            .times(4)
            .returning(|req| {
                let message = generate_message(1, &req.group_id);
                Ok(QueryGroupMessagesResponse {
                    messages: vec![message],
                    paging_info: None,
                })
            });
    }

    #[derive(Default)]
    pub struct CaseState {
        pub current_session: usize,
        pub message_cases: Vec<Vec<MessageTestCase>>,
        pub group_cases: Vec<GroupTestCase>,
        pub sessions: HashMap<usize, StreamSession>,
    }

    fn case_setup(iterator: &[MessageCase]) -> (MockContext, MockProcessFutureFactory) {
        let mut context = context();
        let mut factory = MockProcessFutureFactory::new();
        initial_stream_setup(&mut context);
        let mut state = CaseState::default();
        let mut process_seq = Sequence::new();
        let mut group_sequence = Sequence::new();
        let mut streams = vec![];
        // fill in the 'first' group so we call group_messages at least once
        let groups = vec![GroupTestCase::default()];
        let cases: Vec<MessageTestCase> = iterator
            .iter()
            .copied()
            .map(MessageCase::unwrap_message)
            .collect();
        streams.push(cases.clone());
        for case in &cases {
            if !case.retrieved {
                factory
                    .expect_create()
                    .once()
                    .in_sequence(&mut process_seq)
                    .returning({
                        let case = *case;
                        move |msg| {
                            FutureWrapper::new(future::ready(Ok(ProcessedMessage {
                                message: case
                                    .found
                                    .then(|| generate_stored_msg(msg.id, msg.group_id.clone())),
                                group_id: msg.group_id,
                                next_message: case.next_cursor,
                                tried_to_process: msg.id,
                            })))
                        }
                    });
            } else {
                factory
                    .expect_retrieve()
                    .once()
                    .in_sequence(&mut process_seq)
                    .returning({
                        let case = *case;
                        move |msg| {
                            Ok(case
                                .found
                                .then(|| generate_stored_msg(msg.id, msg.group_id.clone())))
                        }
                    });
            }
        }
        state.message_cases = streams;
        state.group_cases = groups;
        let times = state.group_cases.len();
        let state = Arc::new(Mutex::new(state));

        context
            .api_client
            .api_client
            .expect_subscribe_group_messages()
            .times(times)
            .in_sequence(&mut group_sequence)
            .returning({
                let state = state.clone();
                move |_| {
                    tracing::info!("called");
                    let mut state = state.lock();
                    let cases = state
                        .message_cases
                        .get(state.current_session)
                        .unwrap()
                        .clone();
                    state.current_session += 1;
                    let mut stream = MockGroupStream::new();
                    setup_stream(cases.clone(), &mut stream);
                    Ok(stream)
                }
            });

        (context, factory)
    }

    #[rstest]
    #[case(vec![
        MessageCase::not_found(10, 1, 15),
        MessageCase::not_found(15, 1, 20),
        MessageCase::found(20, 1, 25),
        MessageCase::found(25, 1, 30),
    ], vec![20, 25])]
    #[case(vec![
        MessageCase::found(10, 1, 15),
        MessageCase::not_found(15, 1, 20),
        MessageCase::not_found(20, 1, 25),
        MessageCase::found(25, 1, 30),
    ], vec![10, 25])]
    #[case::nothing_found(vec![
        MessageCase::not_found(10, 1, 15),
        MessageCase::not_found(15, 1, 20),
        MessageCase::not_found(20, 1, 25),
        MessageCase::not_found(25, 1, 30),
    ], vec![])] // should skip work through all not found until end of stream
    #[case::first_is_found(vec![
        MessageCase::found(10, 1, 15),
        MessageCase::not_found(15, 1, 20),
        MessageCase::not_found(20, 1, 25),
        MessageCase::not_found(25, 1, 30),
    ], vec![10])]
    #[case::out_of_order_msgs(vec![
        MessageCase::found(15, 1, 20),
        MessageCase::retrieved(10, 1, true),
        MessageCase::not_found(20, 1, 25),
        MessageCase::found(25, 1, 30),
    ], vec![15, 10, 25])]
    #[case::out_of_order_msgs(vec![
        MessageCase::found(25, 1, 30),
        MessageCase::retrieved(15, 1, true),
        MessageCase::retrieved(10, 1, true),
        MessageCase::retrieved(20, 1, false),
        MessageCase::found(9, 2, 25),
        MessageCase::found(31, 2, 25),
    ], vec![25, 15, 10, 9, 31])]
    #[xmtp_common::test]
    async fn it_can_stream_messages(
        group_list: Vec<GroupId>,
        #[case] cases: Vec<MessageCase>,
        #[case] expected: Vec<u64>,
    ) {
        xmtp_common::logger();
        let (context, factory) = case_setup(&cases);
        let context = Arc::new(context);
        let stream = StreamGroupMessages::new_with_factory(&context, group_list, factory)
            .await
            .unwrap();
        let mut stream = stream.enumerate();
        while let Some((i, Ok(m))) = stream.next().await {
            if let Some(id) = expected.get(i) {
                assert_eq!(m.sequence_id, Some(*id as i64))
            }
        }
    }

    #[rstest]
    #[case(vec![
        StreamSession::init(
            vec![1, 2, 3, 4],
            vec![
                MessageTestCase::found(1, 1, 0),
                MessageTestCase::found(2, 1, 2),
                MessageTestCase::found(3, 1, 3),
                MessageTestCase::found(4, 1, 5),
            ],
            vec![1, 2, 3, 4]
        ),
        StreamSession::session(
            vec![5],
            vec![
                MessageTestCase::found(5, 1, 0),
                MessageTestCase::found(6, 5, 7)
            ],
            vec![5, 6]
        )
    ])]
    #[case(vec![
        StreamSession::init(
            vec![1, 2, 3, 4],
            vec![
                MessageTestCase::found(1, 1, 0),
                MessageTestCase::found(2, 1, 2),
                MessageTestCase::found(3, 1, 3),
                MessageTestCase::found(4, 1, 5),
            ],
            vec![1, 2, 3, 4]
        ),
        StreamSession::session(
            vec![5, 6, 7],
            vec![
                MessageTestCase::not_found(5, 1, 0),
                MessageTestCase::found(6, 1, 0),
                MessageTestCase::found(7, 5, 7)
            ],
            vec![6, 7]
        )
    ])]
    //  #[case(vec![
    //      StreamSession::init(
    //          vec![1, 2, 3, 4],
    //          vec![
    //              MessageTestCase::found(1, 1, 2),
    //              MessageTestCase::found(2, 1, 3),
    //              MessageTestCase::found(3, 1, 4),
    //              MessageTestCase::not_found(4, 1, 5),
    //          ],
    //          vec![1, 2, 3, 4]
    //      ),
    //      StreamSession::session(
    //          vec![5, 6, 7],
    //          vec![
    //              MessageTestCase::not_found(5, 1, 6),
    //              MessageTestCase::found(6, 1, 99),
    //              MessageTestCase::found(7, 5, 7)
    //          ],
    //          vec![6, 7]
    //      )
    //  ])]
    #[xmtp_common::test]
    async fn test_adding_to_stream_works(
        group_list: Vec<GroupId>,
        #[case] cases: Vec<StreamSession>,
    ) {
        use crate::{
            subscriptions::stream_messages::test_case_builder::StreamSequenceBuilder,
            test::mock::MockMlsGroup,
        };
        xmtp_common::logger();
        let mut sequence = StreamSequenceBuilder::default();
        for case in cases.iter().cloned() {
            sequence.session(case);
        }
        let (factory, finished) = sequence.finish();
        let stream = StreamGroupMessages::new_with_factory(&finished.context, group_list, factory)
            .await
            .unwrap();
        let mut stream = std::pin::pin!(stream);

        for session in cases {
            match session {
                StreamSession::Init { expected, .. } => {
                    for exp in expected {
                        let item = stream.next().await.unwrap().unwrap();
                        assert_eq!(item.sequence_id, Some(exp as i64));
                    }
                }
                StreamSession::Session {
                    groups, expected, ..
                } => {
                    for group in groups {
                        stream.as_mut().add(MockMlsGroup::new(
                            finished.context.clone(),
                            group_id(group.group_id).to_vec(),
                            None,
                            xmtp_common::time::now_ns(),
                        ))
                    }
                    for exp in expected.iter() {
                        let item = stream.next().await.unwrap().unwrap();
                        assert_eq!(item.sequence_id, Some(*exp as i64));
                    }
                }
            }
        }
    }

    #[rstest]
    #[case(vec![
        StreamSession::init(
            vec![1, 2, 3, 4],
            vec![
                MessageTestCase::found(1, 1, 2),
                MessageTestCase::found(2, 1, 3),
                MessageTestCase::found(3, 1, 4),
                MessageTestCase::processing_for(4, 1, 5, 3),
            ],
            vec![1, 2, 3]
        ),
        StreamSession::session(
            vec![5],
            vec![
                MessageTestCase::found(5, 1, 0),
                MessageTestCase::found(6, 5, 7)
            ],
            vec![4, 5, 6]
        )
    ])]
    #[case(vec![
        StreamSession::init(
            vec![1, 2, 3, 4],
            vec![
                MessageTestCase::found(1, 1, 2),
                MessageTestCase::found(2, 1, 3),
                MessageTestCase::found(3, 1, 4),
                MessageTestCase::processing_for(4, 1, 5, 3),
            ],
            vec![1, 2, 3]
        ),
        StreamSession::session(
            vec![5, 6, 7, 8],
            vec![
                MessageTestCase::found(5, 1, 99),
                MessageTestCase::processing_for(6, 8, 10, 3),
                MessageTestCase::found(9, 5, 10)
            ],
            vec![4, 5, 6, 9]
        )
    ])]
    #[xmtp_common::test]
    fn it_can_add_to_stream_while_busy(
        group_list: Vec<GroupId>,
        #[case] mut cases: Vec<StreamSession>,
    ) {
        use futures::FutureExt;

        use crate::{
            subscriptions::stream_messages::test_case_builder::StreamSequenceBuilder,
            test::mock::MockMlsGroup,
        };
        xmtp_common::logger();
        let mut sequence = StreamSequenceBuilder::default();
        for case in cases.iter().cloned() {
            sequence.session(case);
        }
        let (factory, finished) = sequence.finish();
        let stream = StreamGroupMessages::new_with_factory(&finished.context, group_list, factory)
            .now_or_never()
            .unwrap()
            .unwrap();
        futures::pin_mut!(stream);

        let noop_waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&noop_waker);

        let cases_length = cases.len();
        for (i, session) in cases.iter_mut().enumerate() {
            match session {
                StreamSession::Init {
                    ref mut expected, ..
                } => {
                    expected.reverse();
                    while !expected.is_empty() {
                        match stream.as_mut().poll_next(&mut cx) {
                            Poll::Ready(Some(Ok(i))) => {
                                tracing::info!("GOT {:?}", i.sequence_id);
                                if let Some(e) = expected.pop() {
                                    assert_eq!(i.sequence_id, Some(e as i64));
                                } else {
                                    break;
                                }
                            }
                            Poll::Ready(None) => {
                                panic!("stream should not finish");
                            }
                            Poll::Pending => {
                                tracing::error!("pending init");
                                continue;
                            }
                            e => panic!("Unexpected {:?}", e),
                        }
                    }
                    tracing::info!("got all messages in expectation for init");
                }
                StreamSession::Session {
                    groups,
                    ref mut expected,
                    ..
                } => {
                    tracing::info!("start new session");
                    // get into busy state
                    let pending = stream.as_mut().poll_next(&mut cx);
                    assert!(pending.is_pending());
                    expected.reverse();
                    for group in groups {
                        stream.as_mut().add(MockMlsGroup::new(
                            finished.context.clone(),
                            group_id(group.group_id).to_vec(),
                            None,
                            xmtp_common::time::now_ns(),
                        ))
                    }
                    loop {
                        match stream.as_mut().poll_next(&mut cx) {
                            Poll::Ready(Some(Ok(i))) => {
                                if let Some(e) = expected.pop() {
                                    assert_eq!(i.sequence_id, Some(e as i64));
                                } else {
                                    break;
                                }
                            }
                            Poll::Ready(None) => {
                                if cases_length >= i {
                                    break;
                                } else {
                                    panic!("stream should not finish");
                                }
                            }
                            Poll::Pending => {
                                tracing::error!("pending session");
                                continue;
                            }
                            e => panic!("Unexpected {:?}", e),
                        }
                    }
                }
            }
        }
    }
}
