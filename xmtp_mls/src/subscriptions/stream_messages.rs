use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
};

use super::{process_message::ProcessMessageFuture, Result, SubscribeError};
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
    /// current message
    cursor: Option<u64>,
}

impl MessagePosition {
    /// Updates the cursor position for this message.
    ///
    /// Sets the cursor to a specific position in the message stream, which
    /// helps track which messages have been processed.
    ///
    /// # Arguments
    /// * `cursor` - The new cursor position to set
    pub(super) fn set(&mut self, cursor: u64) {
        self.cursor = Some(cursor);
    }

    /// Retrieves the current cursor position.
    ///
    /// Returns the cursor position or 0 if no cursor has been set yet.
    ///
    /// # Returns
    /// * `u64` - The current cursor position or 0 if unset
    fn pos(&self) -> u64 {
        self.cursor.unwrap_or(0)
    }
}

impl std::fmt::Display for MessagePosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.pos())
    }
}

impl From<u64> for MessagePosition {
    fn from(v: u64) -> MessagePosition {
        Self { cursor: Some(v) }
    }
}

pin_project! {
    pub struct StreamGroupMessages<'a, ApiClient, Db, Subscription> {
        #[pin] inner: Subscription,
        #[pin] state: State<'a, Subscription>,
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

        tracing::info!("stream_messages ready");
        Ok(Self {
            inner: subscription,
            context,
            state: Default::default(),
            group_list: group_list.into_iter().map(|(g, c)| (g, c.into())).collect(),
            got: Default::default(),
            returned: Default::default(),
            add_queue: Default::default(),
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

impl<'a, ApiClient, Db> Stream
    for StreamGroupMessages<'a, ApiClient, Db, MessagesApiSubscription<'a, ApiClient>>
where
    ApiClient: XmtpApi + XmtpMlsStreams + 'a,
    Db: XmtpDb + 'a,
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

impl<Api, Db, S> StreamGroupMessages<'_, Api, Db, S> {
    fn filters(&self) -> Vec<GroupFilter> {
        self.group_list
            .iter()
            .map(|(group_id, cursor)| GroupFilter::new(group_id.to_vec(), Some(cursor.pos())))
            .collect()
    }
}

impl<'a, Api, Db> StreamGroupMessages<'a, Api, Db, MessagesApiSubscription<'a, Api>>
where
    Api: XmtpApi + XmtpMlsStreams + 'a,
    Db: XmtpDb + 'a,
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
            if m > envelope.id.into() {
                tracing::warn!(
                    "current msg with group_id@[{}], has cursor@[{}]. skipping messages until cursor={m}",
                    xmtp_common::fmt::truncate_hex(hex::encode(
                        envelope.group_id.as_slice()
                    )),
                    envelope.id,
                );
                envelope = ready!(self.as_mut().skip(cx, envelope))?;
            }
            tracing::trace!(
                "group_id {} exists @cursor={m}, proceeding to process message @cursor={}",
                xmtp_common::fmt::truncate_hex(hex::encode(envelope.group_id.as_slice())),
                envelope.id
            );
        }
        let this = self.as_mut().project();
        let msg_cursor = envelope.id;
        let future = ProcessMessageFuture::new(this.context.clone(), envelope);
        let future = future.process();
        let mut this = self.as_mut().project();
        this.state.set(State::Processing {
            future: FutureWrapper::new(future),
            message: msg_cursor,
        });
        cx.waker().wake_by_ref();
        Poll::Pending
        // self.resolve_futures(cx)
    }

    /// Add the group to the group list
    /// and transition the stream to Adding state
    fn resolve_group_additions(mut self: Pin<&mut Self>, group: MlsGroup<Api, Db>) {
        tracing::debug!(
            inbox_id = self.context.inbox_id(),
            installation_id = %self.context.installation_id(),
            group_id = hex::encode(&group.group_id),
            "begin establishing new message stream to include group_id={}",
            hex::encode(&group.group_id)
        );
        let this = self.as_mut().project();
        this.group_list
            .insert(group.group_id.clone().into(), 1.into());
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
                if stream_cursor > new_envelope.id.into() {
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
                if let Some(c) = processed.next_message {
                    self.as_mut().set_cursor(msg.group_id.as_slice(), c);
                }
                tracing::trace!(
                    "returning new message for group=[{}] @cursor=[{:?}], total messages={}",
                    xmtp_common::fmt::debug_hex(msg.group_id.as_slice()),
                    processed.tried_to_process,
                    self.returned.len()
                );
                return Poll::Ready(Some(Ok(msg)));
            } else {
                this.state.set(State::Waiting);
                if let Some(cursor) = this.group_list.get_mut(processed.group_id.as_slice()) {
                    if let Some(c) = processed.next_message {
                        tracing::info!(
                            "no message could be processed, stream setting cursor to [{:?}] for group: {}",
                            processed.next_message,
                            xmtp_common::fmt::truncate_hex(hex::encode(processed.group_id.as_slice()))
                        );

                        cursor.set(c)
                    }
                }
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
mod tests {
    use std::sync::Arc;

    use futures::stream::StreamExt;

    use crate::assert_msg;
    use crate::{builder::ClientBuilder, groups::GroupMetadataOptions};
    use xmtp_cryptography::utils::generate_local_wallet;

    #[rstest::rstest]
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
}
