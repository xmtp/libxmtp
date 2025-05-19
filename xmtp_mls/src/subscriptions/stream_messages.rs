use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
};

use super::{
    process_message::{ProcessMessageFuture, ProcessedMessage},
    Result, SubscribeError,
};
use crate::{
    context::{XmtpContextProvider, XmtpMlsLocalContext},
    groups::MlsGroup,
};
use futures::Stream;
use pin_project_lite::pin_project;
use xmtp_api::GroupFilter;
use xmtp_common::types::GroupId;
use xmtp_common::FutureWrapper;
use xmtp_db::{group_message::StoredGroupMessage, refresh_state::EntityKind, XmtpDb};
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
    }
}

pin_project! {
    #[project = ProjectState]
    #[derive(Default)]
    enum State<'a, Out> {
        /// State that indicates the stream is waiting on the next message from the network
        #[default]
        Waiting,
        /// Replaying messages until a cursor
        Replaying {
            replay_until: u64,
        },
        /// state that indicates the stream is waiting on a IO/Network future to finish processing
        /// the current message before moving on to the next one
        Processing {
            #[pin] future: FutureWrapper<'a, Result<ProcessedMessage>>
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

        Ok(Self {
            inner: subscription,
            context,
            state: Default::default(),
            group_list: group_list.into_iter().map(|(g, c)| (g, c.into())).collect(),
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
        let future = Self::subscribe(&self.context, self.filters(), group.group_id);
        let mut this = self.as_mut().project();
        this.state.set(State::Adding {
            future: FutureWrapper::new(future),
        });
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
    async fn subscribe(
        context: &'a Arc<XmtpMlsLocalContext<ApiClient, Db>>,
        mut filters: Vec<GroupFilter>,
        new_group: Vec<u8>,
    ) -> Result<(MessagesApiSubscription<'a, ApiClient>, Vec<u8>, Option<u64>)> {
        // get the last synced cursor
        let last_cursor = {
            let provider = context.mls_provider();
            provider
                .db()
                .get_last_cursor_for_id(&new_group, EntityKind::Group)
        }?;

        match last_cursor {
            // we dont have messages for the group yet
            0 => {
                let stream = context.api().subscribe_group_messages(filters).await?;
                Ok((stream, new_group, Some(1)))
            }
            c => {
                // should we query for the latest message here instead?
                if let Some(new) = filters.iter_mut().find(|f| f.group_id == new_group) {
                    new.id_cursor = Some(c as u64);
                }
                let stream = context.api().subscribe_group_messages(filters).await?;
                Ok((stream, new_group, Some(c as u64)))
            }
        }
    }
}

impl<'a, ApiClient, Db> Stream
    for StreamGroupMessages<'a, ApiClient, Db, MessagesApiSubscription<'a, ApiClient>>
where
    ApiClient: XmtpApi + XmtpMlsStreams + 'a,
    Db: XmtpDb + 'a,
{
    type Item = Result<StoredGroupMessage>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use ProjectState::*;
        let mut this = self.as_mut().project();
        let state = this.state.as_mut().project();
        match state {
            Waiting => {
                tracing::trace!("stream messages in waiting state");
                self.on_waiting(cx)
            }
            Processing { .. } => {
                tracing::trace!("stream messages in processing state");
                self.resolve_futures(cx)
            }
            Replaying { .. } => {
                tracing::trace!("stream messages in replaying state");
                self.resolve_futures(cx)
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
                self.poll_next(cx)
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
    fn on_waiting(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<<Self as Stream>::Item>> {
        let envelope = ready!(self.as_mut().next_message(cx));
        if envelope.is_none() {
            return Poll::Ready(None);
        }
        let envelope = envelope.expect("checked for none")?;
        // ensure we have not tried processing this message yet
        // if we have tried to process, replay messages up to the known cursor.
        let cursor = self
            .as_ref()
            .group_list
            .get(envelope.group_id.as_slice())
            .copied();
        if let Some(m) = cursor {
            if m > envelope.id.into() {
                tracing::debug!(
                    "current msg with group_id@[{}], has cursor@[{}]. replaying messages until cursor={m}",
                    xmtp_common::fmt::truncate_hex(hex::encode(
                        envelope.group_id.as_slice()
                    )),
                    envelope.id,
                );
                let mut this = self.as_mut().project();
                this.state.set(State::Replaying {
                    replay_until: m.pos(),
                });
            } else {
                tracing::trace!(
                    "group_id {} exists @cursor={m}, proceeding to process message @cursor={}",
                    xmtp_common::fmt::truncate_hex(hex::encode(envelope.group_id.as_slice())),
                    envelope.id
                );
                let this = self.as_mut().project();
                let future = ProcessMessageFuture::new(this.context.clone(), envelope)?;
                let future = future.process();
                let mut this = self.as_mut().project();
                this.state.set(State::Processing {
                    future: FutureWrapper::new(future),
                });
            }
        }
        self.resolve_futures(cx)
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
    fn next_message(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<group_message::V1>>> {
        let this = self.as_mut().project();
        if let Some(envelope) = ready!(this.inner.poll_next(cx)) {
            let envelope = envelope.map_err(|e| SubscribeError::BoxError(Box::new(e)))?;
            if let Some(msg) = extract_message_v1(envelope) {
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

    /// Resolves futures when the stream is in the `Processing` or `Replaying` state.
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
    fn resolve_futures(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<<Self as Stream>::Item>> {
        use ProjectState::*;
        if let Processing { future } = self.as_mut().project().state.project() {
            let processed = ready!(future.poll(cx))?;
            let mut this = self.as_mut().project();
            if let Some(msg) = processed.message {
                this.state.set(State::Waiting);
                self.set_cursor(msg.group_id.as_slice(), processed.next_message);
                return Poll::Ready(Some(Ok(msg)));
            } else {
                this.state.set(State::Waiting);
                let cursor = this.group_list.get_mut(processed.group_id.as_slice());
                if let Some(cursor) = cursor {
                    tracing::info!(
                    "no message could be processed, stream setting cursor to [{}] for group: {}",
                    processed.next_message,
                    xmtp_common::fmt::truncate_hex(hex::encode(processed.group_id.as_slice()))
                );
                    if processed.next_message > cursor.pos() {
                        cursor.set(processed.next_message)
                    }
                }
                return self.poll_next(cx);
            }
        }

        if let Replaying { replay_until } = self.as_mut().project().state.project() {
            let replay: u64 = *replay_until;
            return self.as_mut().resolve_replaying(cx, replay);
        }

        Poll::Pending
    }

    /// Handles message replay to skip already processed messages.
    ///
    /// When the stream detects that a message has a cursor less than what's
    /// already been processed, it enters replay mode. This function skips
    /// messages until reaching the target cursor position.
    ///
    /// # Arguments
    /// * `cx` - The task context for polling
    /// * `replay_until` - The cursor position to replay until
    ///
    /// # Returns
    /// * `Poll<Option<Result<StoredGroupMessage>>>` - The polling result:
    ///   - Returns to normal polling when replay is complete
    ///   - Continues replaying when more messages need to be skipped
    ///
    /// # Note
    /// This function is crucial for handling out-of-order message delivery
    /// and ensuring consistent stream behavior.
    fn resolve_replaying(
        self: &mut Pin<&mut Self>,
        cx: &mut Context<'_>,
        replay_until: u64,
    ) -> Poll<Option<<Self as Stream>::Item>> {
        let envelope = ready!(self.as_mut().next_message(cx));
        if envelope.is_none() {
            return Poll::Ready(None);
        }
        let envelope = envelope.expect("checked for none")?;
        if envelope.id >= replay_until {
            tracing::debug!("finished replaying messages until cursor {replay_until}");
            let mut this = self.as_mut().project();
            this.state.set(State::Waiting);
            return self.as_mut().poll_next(cx);
        }
        self.as_mut().resolve_replaying(cx, replay_until)
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
