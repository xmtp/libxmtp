use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    task::{Context, Poll, ready},
};

use super::{Result, SubscribeError};
use crate::{
    api::GroupFilter,
    groups::{scoped_client::ScopedGroupClient, MlsGroup},
    types::GroupId,
    storage::{
        group::StoredGroup, group_message::StoredGroupMessage, refresh_state::EntityKind,
        StorageError,
    },
    XmtpOpenMlsProvider,
};
use futures::{Stream, TryFutureExt};
use pin_project_lite::pin_project;
use xmtp_common::FutureWrapper;
use xmtp_id::InboxIdRef;
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

fn extract_message_v1(message: GroupMessage) -> Result<group_message::V1> {
    match message.version {
        Some(group_message::Version::V1(value)) => Ok(value),
        _ => Err(MessageStreamError::InvalidPayload.into()),
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
    pub(super) fn set(&mut self, cursor: u64) {
        self.cursor = Some(cursor);
    }

    fn pos(&self) -> u64 {
        self.cursor.unwrap_or(0)
    }

    fn is_unknown(&self) -> bool {
        self.cursor.is_none()
    }
}

impl std::fmt::Display for MessagePosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cursor position {}", self.pos())
    }
}

impl From<StoredGroup> for (Vec<u8>, u64) {
    fn from(group: StoredGroup) -> (Vec<u8>, u64) {
        (group.id, 0u64)
    }
}

impl From<StoredGroup> for (Vec<u8>, MessagePosition) {
    fn from(group: StoredGroup) -> (Vec<u8>, MessagePosition) {
        (group.id, 0u64.into())
    }
}

impl From<u64> for MessagePosition {
    fn from(v: u64) -> MessagePosition {
        Self {
            cursor: Some(v),
        }
    }
}

pin_project! {
    pub struct StreamGroupMessages<'a, C, Subscription> {
        #[pin] inner: Subscription,
        #[pin] state: State<'a, Subscription>,
        client: &'a C,
        group_list: HashMap<GroupId, MessagePosition>,
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
            #[pin] future: FutureWrapper<'a, Result<Option<(StoredGroupMessage, u64)>>>
        },
        Adding {
            #[pin] future: FutureWrapper<'a, Result<Out>>
        }
    }
}

pub(super) type MessagesApiSubscription<'a, C> =
    <<C as ScopedGroupClient>::ApiClient as XmtpMlsStreams>::GroupMessageStream<'a>;

impl<'a, C> StreamGroupMessages<'a, C, MessagesApiSubscription<'a, C>>
where
    C: ScopedGroupClient + 'a,
    <C as ScopedGroupClient>::ApiClient: XmtpApi + XmtpMlsStreams + 'a,
{
    pub async fn new(
        client: &'a C,
        group_list: Vec<GroupId>,
    ) -> Result<Self> {
        tracing::debug!("setting up messages subscription");
        let group_list = group_list.into_iter().map(|group_id| {
            Ok((group_id, 0u64))
        }).collect::<Result<HashMap<GroupId, u64>>>()?;
        let filters: Vec<GroupFilter> = group_list
            .iter()
            .map(|(group_id, cursor)| GroupFilter::new(group_id.to_vec(), Some(*cursor)))
            .collect();
        for filter in &filters {
            let messages = client.api().query_group_messages(filter.group_id.to_vec(), Some(1)).await;
            tracing::info!("{:?}", messages);
            tracing::debug!("Subscribing to {} for group messages", hex::encode(&filter.group_id));
        }
        let subscription = client.api().subscribe_group_messages(filters).await?;

        Ok(Self {
            inner: subscription,
            client,
            state: Default::default(),
            group_list: group_list.into_iter().map(|(g, c)| (g, c.into())).collect(),
        })
    }

    /// Add a new group to this messages stream
    pub(super) fn add(mut self: Pin<&mut Self>, group: MlsGroup<C>) {
        tracing::info!("creating new messages stream to add group {}", hex::encode(&group.group_id));
        if self.group_list.contains_key(group.group_id.as_slice()) {
            tracing::info!("group {} already in stream", hex::encode(&group.group_id));
            return;
        }

        tracing::debug!(
            inbox_id = self.client.inbox_id(),
            installation_id = %self.client.installation_id(),
            group_id = hex::encode(&group.group_id),
            "begin establishing new message stream to include group_id={}",
            hex::encode(&group.group_id)
        );
        let this = self.as_mut().project();
        this.group_list.insert(group.group_id.into(), 1.into());
        // let mut filters = self.filters();
        // add the new group but not to our state.
        // We will add the group to our state once we get the first message.
        // In that message will be the real cursor, rather than a temporary `1`
        // filters.push(GroupFilter::new(group.group_id, Some(1)));
        let future = self.client.api().subscribe_group_messages(self.filters()).map_err(SubscribeError::from);
        let mut this = self.as_mut().project();
        this.state.set(State::Adding { future: FutureWrapper::new(future)});
    }

    // Reinit with all the correct cursors
    // this should result in the least amount of network calls & mitigate missed messages to get the right messages
    // when groups are changing quickly
    // TODO: can store a cursor on a message, or in refresh table, that is last processed message
    // not necessarily synced. then we don't need this.
    fn reinit(mut self: Pin<&mut Self>) {
        let future = self.client.api().subscribe_group_messages(self.filters()).map_err(SubscribeError::from);
        let mut this = self.as_mut().project();
        this.state.set(State::Adding { future: FutureWrapper::new(future)});
    }
}

impl<'a, C> Stream for StreamGroupMessages<'a, C, MessagesApiSubscription<'a, C>>
where
    C: ScopedGroupClient + 'a,
    <C as ScopedGroupClient>::ApiClient: XmtpApi + XmtpMlsStreams + 'a,
{
    type Item = Result<StoredGroupMessage>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // tracing::debug!("POLLING STREAM MESSAGES");
        use std::task::Poll::*;
        use ProjectState::*;
        let mut this = self.as_mut().project();

        match this.state.as_mut().project() {
            Waiting =>  {
                if let Some(envelope) = ready!(this.inner.poll_next(cx)) {
                    tracing::debug!("processing {:?} message in stream", envelope);
                    let future = ProcessMessageFuture::new(*this.client, envelope?)?;
                    let future = future.process();
                    this.state.set(State::Processing {
                        future: FutureWrapper::new(future),
                    });
                    self.try_update_state(cx)
                } else { // the stream ended
                    Ready(None)
                }
            },
            Processing { .. } => self.try_update_state(cx),
            Adding { future } => {
                let stream = ready!(future.poll(cx))?;
                let mut this = self.as_mut().project();
                this.inner.set(stream);
                this.state.as_mut().set(State::Waiting);
                tracing::debug!("added group to messages stream");
                self.poll_next(cx)
            }
        }
    }
}

impl<'a, C, S> StreamGroupMessages<'a, C, S> {
    fn filters(&self) -> Vec<GroupFilter> {
        self.group_list
            .iter()
            .map(|(group_id, cursor)| GroupFilter::new(group_id.to_vec(), Some(cursor.pos())))
            .collect()
    }
}

impl<'a, C> StreamGroupMessages<'a, C, MessagesApiSubscription<'a, C>>
where
    C: ScopedGroupClient + 'a,
    // Subscription: Stream<Item = std::result::Result<GroupMessage, xmtp_proto::Error>> + 'a,
    <C as ScopedGroupClient>::ApiClient: XmtpApi + XmtpMlsStreams + 'a,
{
    /// Try to finish processing the stream item by polling the stored future.
    /// Update state to `Waiting` and insert the new cursor if ready.
    /// If Stream state is in `Waiting`, returns `Pending`.
    fn try_update_state(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<<Self as Stream>::Item>> {
        use ProjectState::*;

        let mut this = self.as_mut().project();
        if let Processing { future } = this.state.as_mut().project() {
            match ready!(future.poll(cx))? {
                Some((msg, new_cursor)) => {
                    this.state.set(State::Waiting);
                    if let Some(tracked_cursor) = this.group_list.get_mut(msg.group_id.as_slice()) {
                        if tracked_cursor.is_unknown() { // we assume a cursor of 1 means unknown cursor
                            // reinit the stream with the correct cursor
                            tracing::info!("reinit");
                            tracked_cursor.set(new_cursor);
                            self.as_mut().reinit();
                            return Poll::Pending;
                            // return self.poll_next(cx);
                        } else {
                            tracked_cursor.set(new_cursor);
                          return Poll::Ready(Some(Ok(msg)));
                         }
                    } else {
                        tracing::info!("\n\nGot new group\n\n");
                        this.group_list
                            .insert(msg.group_id.clone().into(), new_cursor.into());
                        return self.poll_next(cx);
                    }
                    // return Poll::Ready(Some(Ok(msg)));
                },
                None => {
                    tracing::warn!("skipping message streaming payload");
                    this.state.set(State::Waiting);
                    // we are skipping this message and need to add the task
                    // back to the queue to start polling for the next one
                    return self.poll_next(cx);
                    // cx.waker().wake_by_ref();
                    // return Poll::Pending;
                }
            }
        }
        Poll::Pending
    }
}

impl<'a, C> StreamGroupMessages<'a, C, MessagesApiSubscription<'a, C>>
where
    // S: Stream<Item = std::result::Result<GroupMessage, xmtp_proto::Error>> + 'a,
    C: ScopedGroupClient + 'a,
    <C as ScopedGroupClient>::ApiClient: XmtpApi + XmtpMlsStreams + 'a,
{
    pub(super) fn drain(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) {
        let mut this = self.as_mut().project();
        // let mut drained = Vec::new();
        while let Poll::Ready(msg) = this.inner.as_mut().poll_next(cx) {
            tracing::info!("msg: {:?}", msg);
        }
    }
}

/// Future that processes a group message from the network
pub struct ProcessMessageFuture<Client> {
    provider: XmtpOpenMlsProvider,
    client: Client,
    msg: group_message::V1,
}

impl<C> ProcessMessageFuture<C>
where
    C: ScopedGroupClient,
{
    /// Create a new Future to process a GroupMessage.
    pub fn new(client: C, envelope: GroupMessage) -> Result<ProcessMessageFuture<C>> {
        let msg = extract_message_v1(envelope)?;
        let provider = client.mls_provider()?;
        tracing::info!(
            inbox_id = client.inbox_id(),
            group_id = hex::encode(&msg.group_id),
            "Received message streaming payload"
        );

        Ok(Self {
            provider,
            client,
            msg,
        })
    }

    fn inbox_id(&self) -> InboxIdRef<'_> {
        self.client.inbox_id()
    }

    /// process a message, returning the message from the database and the cursor of the message.
    pub(crate) async fn process(self) -> Result<Option<(StoredGroupMessage, u64)>> {
        let group_message::V1 {
            // the cursor ID is the position in the monolithic backend topic
            id: ref cursor_id,
            ref created_ns,
            ..
        } = self.msg;

        tracing::info!(
            inbox_id = self.inbox_id(),
            group_id = hex::encode(&self.msg.group_id),
            cursor_id,
            "client [{}]  is about to process streamed envelope: [{}]",
            self.inbox_id(),
            &cursor_id
        );

        if self.needs_to_sync(*cursor_id)? {
            self.process_stream_entry().await
        }

        // Load the message from the DB to handle cases where it may have been already processed in
        // another thread
        let new_message = self
            .provider
            .conn_ref()
            .get_group_message_by_timestamp(&self.msg.group_id, *created_ns as i64)?;
            /*
            .inspect(|e| {
                if matches!(e, SubscribeError::GroupMessageNotFound) {
                    tracing::warn!(
                        cursor_id,
                        inbox_id = self.inbox_id(),
                        group_id = hex::encode(&self.msg.group_id),
                        "group message not found"
                    );
                }
            })?;
        */
        if let Some(msg) = new_message {
            Ok(Some((msg, *cursor_id)))
        } else {
            tracing::warn!(
                cursor_id,
                inbox_id = self.inbox_id(),
                group_id = hex::encode(&self.msg.group_id),
                "group message not found"
            );

            Ok(None)
        }
    }

    /// stream processing function
    async fn process_stream_entry(&self) {
        let process_result = self
            .client
            .store()
            .retryable_transaction_async(&self.provider, |provider| async move {
                let (group, _) =
                    MlsGroup::new_validated(&self.client, self.msg.group_id.clone(), provider)?;
                tracing::info!(
                    inbox_id = self.inbox_id(),
                    group_id = hex::encode(&self.msg.group_id),
                    cursor_id = self.msg.id,
                    "current epoch for [{}] in process_stream_entry()",
                    self.inbox_id(),
                );
                group
                    .process_message(provider, &self.msg, false)
                    .await
                    // NOTE: We want to make sure we retry an error in process_message
                    .map_err(SubscribeError::ReceiveGroup)
            })
            .await;

        if let Err(SubscribeError::ReceiveGroup(e)) = process_result {
            tracing::warn!("error processing streamed message {e}");
            self.attempt_message_recovery().await
        // This should never occur because we map the error to `ReceiveGroup`
        // But still exists defensively
        } else if let Err(e) = process_result {
            tracing::error!(
                inbox_id = self.client.inbox_id(),
                group_id = hex::encode(&self.msg.group_id),
                cursor_id = self.msg.id,
                err = e.to_string(),
                "process stream entry {:?}",
                e
            );
        } else {
            tracing::trace!(
                cursor_id = self.msg.id,
                inbox_id = self.inbox_id(),
                group_id = hex::encode(&self.msg.group_id),
                "message process in stream success"
            );
        }
    }

    /// Checks if a message has already been processed through a sync
    // TODO: Make this not async, and instead of retry add it back to wake queue.
    fn needs_to_sync(&self, current_msg_cursor: u64) -> Result<bool> {
        let check_for_last_cursor = || -> std::result::Result<i64, StorageError> {
            self.provider
                .conn_ref()
                .get_last_cursor_for_id(&self.msg.group_id, EntityKind::Group)
        };

        let last_synced_id = check_for_last_cursor()?;
        Ok(last_synced_id < current_msg_cursor as i64)
    }

    /// Attempt a recovery sync if a group message failed to process
    async fn attempt_message_recovery(&self) {
        let group = MlsGroup::new(
            &self.client,
            self.msg.group_id.clone(),
            self.msg.created_ns as i64,
        );
        tracing::debug!(
            inbox_id = self.client.inbox_id(),
            group_id = hex::encode(&self.msg.group_id),
            cursor_id = self.msg.id,
            "attempting recovery sync"
        );
        // Swallow errors here, since another process may have successfully saved the message
        // to the DB
        if let Err(err) = group.sync_with_conn(&self.provider).await {
            tracing::warn!(
                inbox_id = self.client.inbox_id(),
                group_id = hex::encode(&self.msg.group_id),
                cursor_id = self.msg.id,
                err = %err,
                "recovery sync triggered by streamed message failed: {}", err
            );
        } else {
            tracing::debug!(
                inbox_id = self.client.inbox_id(),
                group_id = hex::encode(&self.msg.group_id),
                cursor_id = self.msg.id,
                "recovery sync triggered by streamed message successful"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures::stream::StreamExt;
    use wasm_bindgen_test::wasm_bindgen_test;

    use crate::{assert_msg, assert_msg_exists};
    use crate::{builder::ClientBuilder, groups::GroupMetadataOptions};
    use xmtp_cryptography::utils::generate_local_wallet;

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_stream_messages() {
        xmtp_common::logger();
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
        let bob_groups = bob
            .sync_welcomes(&bob.mls_provider().unwrap())
            .await
            .unwrap();
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
