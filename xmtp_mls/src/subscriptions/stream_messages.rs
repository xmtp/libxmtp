#[cfg(test)]
mod test_case_builder;
#[cfg(test)]
mod test_utils;
#[cfg(test)]
mod unit_tests;

mod state;
mod types;
mod versioned_messages_stream;

pub use state::StateError;
use types::GroupList;
pub(super) use types::MessagePosition;
pub use types::MessageStreamError;
use versioned_messages_stream::*;

use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
};

use super::{
    process_message::{self, ProcessMessage},
    Result,
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
    pub struct StreamGroupMessages<'a, ApiClient, Db, Subscription, Factory = ProcessMessage<ApiClient, Db>> {
        #[pin] inner: VersionedMessagesStream<Subscription>,
        #[pin] state: state::State<'a, Subscription>,
        factory: Factory,
        context: &'a Arc<XmtpMlsLocalContext<ApiClient, Db>>,
        groups: GroupList,
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

pub(super) type MessagesApiSubscription<ApiClient> =
    <ApiClient as XmtpMlsStreams>::GroupMessageStream;

impl<'a, ApiClient, Db> StreamGroupMessages<'a, ApiClient, Db, MessagesApiSubscription<ApiClient>>
where
    ApiClient: XmtpApi + XmtpMlsStreams + 'a,
    Db: XmtpDb + 'a,
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
    pub async fn new(
        context: &'a Arc<XmtpMlsLocalContext<ApiClient, Db>>,
        groups: Vec<GroupId>,
    ) -> Result<Self> {
        Self::new_with_factory(context, groups, ProcessMessage::new(context.clone())).await
    }
}

#[cfg(test)]
impl<'a, ApiClient, Db, S> StreamGroupMessages<'a, ApiClient, Db, S> {
    pub fn position(&self, group: impl AsRef<[u8]>) -> Option<MessagePosition> {
        self.groups.position(group)
    }
}

impl<'a, ApiClient, Db, Factory>
    StreamGroupMessages<'a, ApiClient, Db, MessagesApiSubscription<ApiClient>, Factory>
where
    ApiClient: XmtpApi + XmtpMlsStreams + 'a,
    Db: XmtpDb + 'a,
    Factory: process_message::Factory<'a> + 'a,
{
    pub async fn new_with_factory(
        context: &'a Arc<XmtpMlsLocalContext<ApiClient, Db>>,
        groups: Vec<GroupId>,
        factory: Factory,
    ) -> Result<Self> {
        tracing::debug!("setting up messages subscription");
        let api = context.api();
        let groups = GroupList::new(groups, api).await?;
        let subscription = api.subscribe_group_messages(groups.filters()).await?;
        let subscription = VersionedMessagesStream::new(subscription);
        tracing::info!("stream_messages ready");

        Ok(Self {
            inner: subscription,
            context,
            state: state::State::default(),
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
    pub(super) fn add(mut self: Pin<&mut Self>, group: MlsGroup<ApiClient, Db>) {
        if self.groups.contains(&group.group_id) {
            tracing::debug!("group {} already in stream", hex::encode(&group.group_id));
            return;
        }
        // add group to the add queue
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
    /// * `filters` - Current list of group filters
    /// * `new_group` - ID of the new group to add
    ///
    /// # Returns
    /// * `Result<(MessagesApiSubscription<C>, Vec<u8>, Option<u64>)>` - A tuple containing:
    ///   - The new message subscription
    ///   - The ID of the newly added group
    ///   - The cursor position for the new group (if available)
    ///
    /// # Errors
    /// May return errors if:
    /// - Querying the database for the last cursor fails
    /// - Creating the new subscription fails
    #[tracing::instrument(level = "trace", skip(context, new_group), fields(new_group = hex::encode(&new_group)))]
    async fn subscribe(
        context: &'a Arc<XmtpMlsLocalContext<ApiClient, Db>>,
        filters: Vec<GroupFilter>,
        new_group: Vec<u8>,
    ) -> Result<(MessagesApiSubscription<ApiClient>, Vec<u8>, Option<u64>)> {
        // get the last synced cursor
        let stream = context.api().subscribe_group_messages(filters).await?;
        Ok((stream, new_group, Some(1)))
    }
}

impl<'a, ApiClient, Db, Factory> Stream
    for StreamGroupMessages<'a, ApiClient, Db, MessagesApiSubscription<ApiClient>, Factory>
where
    ApiClient: XmtpApi + XmtpMlsStreams + 'a,
    Db: XmtpDb + 'a,
    Factory: process_message::Factory<'a> + 'a,
{
    type Item = Result<StoredGroupMessage>;

    #[tracing::instrument(level = "trace", skip_all, name = "poll_next_message")]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let transition = ready!(self.as_mut().project().state.poll(cx))?;
        if let Some(msg) = transition.apply_to(self.as_mut()) {
            return Poll::Ready(Some(Ok(msg)));
        }

        let this = self.as_mut().project();
        // handle any queued groups
        if let Some(group) = this.add_queue.pop_front() {
            self.as_mut().add_new_group(group)?;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        todo!()

        // try to resolve the next message

        /*
        use ProjectState::*;
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
        }
        */
    }
}

impl<'a, Api, Db, Factory> StreamGroupMessages<'a, Api, Db, MessagesApiSubscription<Api>, Factory>
where
    Api: XmtpApi + XmtpMlsStreams + 'a,
    Db: XmtpDb + 'a,
    Factory: process_message::Factory<'a> + 'a,
{
    /*
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
            let next_msg = ready!(self.as_mut().next_message(cx));
            if next_msg.is_none() {
                return Poll::Ready(None);
            }
            let mut next_msg = next_msg.expect("checked for none")?;
            // ensure we have not tried processing this message yet
            // if we have tried to process, replay messages up to the known cursor.
            let cursor = self.groups.position(&next_msg.group_id);
            if let Some(position) = cursor {
                if position.last_streamed() > next_msg.id && position.started() > next_msg.id {
                    tracing::warn!(
                        "stream started @[{}] has cursor@[{}] for group_id@[{}], skipping messages for msg with cursor@[{}]",
                        position.started(),
                        position.last_streamed(),
                        xmtp_common::fmt::truncate_hex(hex::encode(
                            next_msg.group_id.as_slice()
                        )),
                        next_msg.id,
                    );
                    next_msg = ready!(self.as_mut().skip(cx, next_msg))?;
                // we got a message with a sequence_id greater than a message we already processed
                // so it must be present in the database
                } else if position.last_streamed() > next_msg.id && position.started() < next_msg.id {
                    tracing::debug!(
                        "stream synced up to cursor@[{}], checking for message with cursor@[{}] in database",
                        position.last_streamed(),
                        next_msg.id
                    );
                    if let Some(stored) = self.factory.retrieve(&next_msg)? {
                        return Poll::Ready(Some(Ok(stored)));
                    } else {
                        cx.waker().wake_by_ref();
                        return Poll::Pending;
                    }
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
    */

    /// Add the group to the group list
    /// and transition the stream to Adding state
    fn add_new_group(mut self: Pin<&mut Self>, group: MlsGroup<Api, Db>) -> Result<()> {
        let this = self.as_mut().project();
        tracing::debug!(
            "adding group_id={} to message stream",
            hex::encode(&group.group_id)
        );
        this.groups.add(&group.group_id, MessagePosition::new(1, 1));
        this.state
            .resubscribe(this.context.api(), &this.groups, group.group_id.into())?;
        Ok(())
    }
    /*
    // iterative skip to avoid overflowing the stack
    fn skip(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut envelope: group_message::V1,
    ) -> Poll<Result<group_message::V1>> {
        // skip the messages
        while let Some(new_envelope) = ready!(self.as_mut().next_message(cx)) {
            let new_envelope = new_envelope?;
            if let Some(stream_cursor) = self.as_ref().groups.position(&new_envelope.group_id) {
                if stream_cursor.last_streamed() > new_envelope.id {
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

    */
}

impl<'a, A, D, S, F> StreamGroupMessages<'a, A, D, S, F> {
    /// Updates the cursor position for a specific group.
    ///
    /// This method updates the tracking information for a group after
    /// successfully processing a message, allowing the stream to maintain
    /// proper ordering and prevent duplicate processing.
    ///
    /// # Arguments
    /// * `group_id` - The ID of the group to update
    /// * `new_cursor` - The new cursor position to set
    fn set_cursor(&mut self, group_id: impl AsRef<[u8]>, new_cursor: u64) {
        self.groups.set(group_id, new_cursor);
    }
}

#[cfg(test)]
pub mod tests {
    use std::sync::Arc;

    use futures::stream::StreamExt;

    use crate::assert_msg;
    use crate::builder::ClientBuilder;
    use rstest::*;
    use xmtp_cryptography::utils::generate_local_wallet;

    #[rstest]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(5))]
    #[cfg_attr(target_arch = "wasm32", ignore)]
    async fn test_stream_messages() {
        let alice = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bob = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alice_group = alice.create_group(None, None).unwrap();
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
